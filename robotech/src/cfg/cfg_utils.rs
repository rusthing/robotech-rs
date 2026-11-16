use crate::cfg::base_config::BaseConfig;
use crate::cfg::cfg_error::CfgError;
#[cfg(feature = "config-center")]
use crate::micro_svc::get_configs;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{get_hub_client, setup_hub_client, MicroSvcConfig, MICRO_SVC_CONFIG_KEY};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use std::path::Path;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
use tracing::{debug, warn};

/// # 配置模块的 Result 别名
///
/// 配置操作统一返回的错误类型，错误变体为 [`CfgError`]。
pub type Result<T> = core::result::Result<T, CfgError>;

/// # 构建应用配置
///
/// 依次加载基础配置文件、profile 对应的配置文件（可选），并按需从配置中心
/// 拉取配置，最后叠加环境变量覆盖，构建完整的配置对象。
///
/// ## 参数
/// * `app_dir` - 应用所在目录，用于定位默认配置文件
/// * `env_var_prefix` - 环境变量前缀，用于从环境变量中覆盖配置
/// * `app_file_name_without_ext` - 应用文件名（不含扩展名）；为 `None` 时表示仅构建日志配置，
///   不进行配置中心初始化
/// * `cfg_file_name_without_ext` - 配置文件基础名（不含扩展名），如 `"config"`
/// * `cfg_file_path` - 可选的显式配置文件路径，指定后不再按目录查找默认配置
///
/// ## 返回值
/// 返回构建完成的 `Config` 对象与已加载的配置文件路径列表
///
/// ## 错误
/// 配置文件构建失败时返回 `CfgError::Build`；反序列化失败时返回 `CfgError::Deserialize`
pub async fn build_cfg(
    app_dir: &Path,
    env_var_prefix: &str,
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    app_file_name_without_ext: Option<&str>,
    #[cfg(not(any(feature = "config-center", feature = "registry-center")))]
    _app_file_name_without_ext: Option<&str>,
    cfg_file_name_without_ext: &str,
    cfg_file_path: Option<String>,
) -> Result<(Config, Vec<String>)> {
    // 先加载基础配置文件获取profile，后续根据profile加载对应的配置文件
    let config_builder = Config::builder();
    let (config_builder, ..) = add_cfg_files(
        app_dir,
        cfg_file_name_without_ext,
        &cfg_file_path,
        config_builder,
    )?;
    let BaseConfig {
        app_name, profile, ..
    } = config_builder
        .build()
        .map_err(CfgError::Build)?
        .try_deserialize()
        .map_err(CfgError::Deserialize)?;

    if let Some(name) = &app_name {
        crate::env::set_app_name(name.clone());
    }

    let config_builder = Config::builder();

    // 加载配置文件
    let (config_builder, mut files) = add_cfg_files(
        app_dir,
        cfg_file_name_without_ext,
        &cfg_file_path,
        config_builder,
    )?;

    // 加载profile对应的配置文件
    let (mut config_builder, files) = if let Some(profile) = &profile {
        let (config_builder, profile_files) = add_cfg_files(
            app_dir,
            format!("{}-{}", cfg_file_name_without_ext, profile).as_str(),
            &cfg_file_path,
            config_builder,
        )?;
        files.extend(profile_files);
        (config_builder, files)
    } else {
        (config_builder, files)
    };

    // 初始化配置中心和注册中心的客户端
    // 如果传入app_file_name_without_ext为None，说明是构建log配置，不需要通过配置中心初始化配置
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    if let Some(app_file_name_without_ext) = app_file_name_without_ext {
        // 初始化配置中心和注册中心的客户端
        init_hub_client(
            config_builder.clone(),
            app_name.unwrap_or(app_file_name_without_ext.to_string()),
            &profile,
        )
        .await?;
        // 从配置中心获取配置文件内容并加载到config中。
        // 只有 hub client 初始化成功（即真的配了 `[micro-svc]` 并连上）才拉取，
        // 否则每次都会拿到 NotInit 错误并打一条无意义的 warn。
        #[cfg(feature = "config-center")]
        if get_hub_client().is_err() {
            // 没配配置中心，跳过
        } else {
            match get_configs().await {
                Ok(config_keys) => {
                    for item in config_keys {
                        // 如果是 refresh-scope key，直接以 TOML 格式注入到 [refresh-scope] 段落下
                        if let Ok(hub_client) = get_hub_client() {
                            if hub_client.is_refresh_scope_key(&item.key.data_id) {
                                let wrapped = format!("[refresh-scope]\n{}", item.content);
                                config_builder = config_builder.add_source(config::File::from_str(
                                    &wrapped,
                                    config::FileFormat::Toml,
                                ));
                                continue;
                            }
                        }
                        config_builder = config_builder
                            .add_source(config::File::from_str(&item.content, item.format));
                    }
                }
                Err(e) => {
                    warn!("Failed to get configs from config center: {:?}", e);
                }
            }
        }
    }

    // 添加环境变量，以覆盖配置文件中的设置
    config_builder = config_builder
        // Add in app from the environment (with a prefix of XXX)
        // E.g. `XXX_DEBUG=true ./target/app` would set the `debug` to `true`
        .add_source(config::Environment::with_prefix(env_var_prefix));

    let config = config_builder.build().map_err(CfgError::Build)?;

    Ok((config, files))
}

/// # 反序列化配置对象
///
/// 将构建完成的 `Config` 反序列化为指定的配置结构体类型。
///
/// ## 参数
/// * `config` - 待反序列化的配置对象
///
/// ## 返回值
/// 返回反序列化后的配置结构体
///
/// ## 错误
/// 配置反序列化失败时返回 `CfgError::Deserialize`
pub async fn deserialize_config<'a, T>(config: Config) -> Result<T>
where
    T: serde::Deserialize<'a>,
{
    config.try_deserialize().map_err(CfgError::Deserialize)
}

#[cfg(any(feature = "config-center", feature = "registry-center"))]
async fn init_hub_client(
    config: ConfigBuilder<DefaultState>,
    app_name: String,
    profile: &Option<String>,
) -> Result<()> {
    // 如果 micro-svc 没配置，直接返回 None，跳过 hub client 初始化
    let mut micro_svc_config: MicroSvcConfig = match config
        .build()
        .map_err(CfgError::Build)?
        .get(MICRO_SVC_CONFIG_KEY)
    {
        Ok(value) => value,
        // 没配 `[micro-svc]` 是常态（网关、纯本地服务都不需要服务治理），
        // 不是错误。用 debug 级别，否则每次启动和每次热更新都会刷一条 WARN。
        Err(config::ConfigError::NotFound(_)) => {
            debug!("未配置 [{MICRO_SVC_CONFIG_KEY}]，跳过服务治理客户端初始化");
            return Ok(());
        }
        // 配了但写错才是真问题，保持 warn
        Err(e) => {
            warn!("[{MICRO_SVC_CONFIG_KEY}] 解析失败: {:?}", e);
            return Ok(());
        }
    };
    if micro_svc_config.svc_name.is_none() {
        micro_svc_config.svc_name = Some(app_name.to_string());
    }
    if micro_svc_config.profile.is_none() {
        micro_svc_config.profile = profile.clone();
    }

    // micro-svc 段没有变化时复用现有客户端。
    //
    // 配置热更新本身会走一遍 build_cfg，如果这里无条件 setup_hub_client，
    // 每次改任何一个配置项都会：注销服务实例 → 重新注册 → 中断所有配置 watch。
    // 服务发现会出现短暂空洞，watch 也要等 reload 完成后才重新挂上。
    if let Ok(hub_client) = get_hub_client() {
        if hub_client.matches_micro_svc_config(&micro_svc_config) {
            return Ok(());
        }
    }

    setup_hub_client(micro_svc_config).await;
    Ok(())
}

/// # 加载配置文件
///
/// 如果指定了配置文件路径，加载该文件；否则，根据应用目录和配置文件名加载默认配置文件。
///
/// 支持的配置文件格式：toml, json, json5, yml, yaml, ini, ron
fn add_cfg_files(
    app_dir: &Path,
    cfg_file_name_without_ext: &str,
    cfg_file_path: &Option<String>,
    mut config: ConfigBuilder<DefaultState>,
) -> Result<(ConfigBuilder<DefaultState>, Vec<String>)> {
    let mut files = vec![];
    // 如果已指定配置文件路径
    if let Some(cfg_file_path) = cfg_file_path.clone() {
        files.push(cfg_file_path.clone());
        let config = add_source(config, Path::new(cfg_file_path.as_str()));
        return Ok((config, files));
    }

    for ext in ["toml", "json", "json5", "yml", "yaml", "ini", "ron"] {
        // 两个候选：`$APP_DIR/config/<name>.<ext>` 与 `$APP_DIR/<name>.<ext>`。
        //
        // `config/` 优先是有原因的：容器里若用「单文件 bind」挂配置，容器内路径会被
        // 钉在启动时的 inode 上，host 侧 rsync / 编辑器原子替换后读到的仍是旧文件；
        // 改成「整目录挂载」后真正的文件在 `$APP_DIR/config/` 下，这里直接认它，
        // 就不需要再靠 init.sh 建软链把路径摆回应用根目录。
        let in_config_dir = app_dir
            .join("config")
            .join(format!("{cfg_file_name_without_ext}.{ext}"));
        let in_app_dir = app_dir.join(format!("{cfg_file_name_without_ext}.{ext}"));

        // 两个候选都登记监听（不存在也登记），后续往任一位置补文件都能热更新；
        // 但只加载先命中的那个，避免两份同名文件叠加成难以解释的结果。
        files.push(in_config_dir.to_string_lossy().to_string());
        files.push(in_app_dir.to_string_lossy().to_string());

        let picked = if in_config_dir.exists() {
            Some(&in_config_dir)
        } else if in_app_dir.exists() {
            Some(&in_app_dir)
        } else {
            None
        };
        if let Some(path) = picked {
            config = add_source(config, path);
        }
    }
    Ok((config, files))
}

/// # 向配置构建器添加单个配置源
///
/// 根据文件路径（可含扩展名）构建 `config::File` 并添加到配置构建器。
/// `.json` 后缀的文件会以 Json5 格式加载。
///
/// 只负责「加载」：路径登记与监听由调用方（[`add_cfg_files`]）统一处理，
/// 因此文件不存在时直接原样返回，不报错。
///
/// ## 参数
/// * `config` - 配置构建器
/// * `file_path` - 配置文件路径（含扩展名）
///
/// ## 返回值
/// 返回添加了配置源的构建器；文件不存在时原样返回
fn add_source(
    config: ConfigBuilder<DefaultState>,
    file_path: &Path,
) -> ConfigBuilder<DefaultState> {
    if !file_path.exists() {
        return config;
    }
    let mut file = config::File::with_name(file_path.to_string_lossy().as_ref());
    // .json 后缀也用 Json5 格式
    if file_path
        .extension()
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        file = file.format(config::FileFormat::Json5)
    }
    config.add_source(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_app_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("robotech_cfg_test_{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn config_dir_wins_over_app_dir() {
        let app_dir = temp_app_dir("config_first");
        std::fs::create_dir_all(app_dir.join("config")).unwrap();
        std::fs::write(app_dir.join("config/log.toml"), "level = \"debug\"\n").unwrap();
        std::fs::write(app_dir.join("log.toml"), "level = \"info\"\n").unwrap();

        let (builder, files) = add_cfg_files(&app_dir, "log", &None, Config::builder()).unwrap();
        let cfg = builder.build().unwrap();
        // config/ 优先：加载的是 config/log.toml
        assert_eq!(cfg.get_string("level").unwrap(), "debug");
        // 两个候选都登记监听
        assert!(files.iter().any(|f| f.ends_with("config/log.toml")));
        assert!(files.iter().any(|f| f.ends_with("/log.toml")));
        std::fs::remove_dir_all(&app_dir).ok();
    }

    #[test]
    fn falls_back_to_app_dir_and_registers_missing_config_dir() {
        let app_dir = temp_app_dir("app_fallback");
        std::fs::write(app_dir.join("log.toml"), "level = \"warn\"\n").unwrap();

        let (builder, files) = add_cfg_files(&app_dir, "log", &None, Config::builder()).unwrap();
        let cfg = builder.build().unwrap();
        assert_eq!(cfg.get_string("level").unwrap(), "warn");
        // config/ 下没文件时仍登记，便于之后往那里补文件
        assert!(files.iter().any(|f| f.ends_with("config/log.toml")));
        std::fs::remove_dir_all(&app_dir).ok();
    }
}
