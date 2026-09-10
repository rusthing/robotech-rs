use crate::cfg::base_config::BaseConfig;
use crate::cfg::cfg_error::CfgError;
#[cfg(feature = "config-center")]
use crate::micro_svc::get_configs;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{setup_hub_client, MicroSvcConfig, MICRO_SVC_CONFIG_KEY};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use std::path::{Path, PathBuf};
#[cfg(any(feature = "config-center", feature = "registry-center"))]
use tracing::warn;

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
    app_dir: &PathBuf,
    env_var_prefix: &str,
    app_file_name_without_ext: Option<&str>,
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
        #[cfg(any(feature = "config-center", feature = "registry-center"))]
        init_hub_client(
            config_builder.clone(),
            app_name.unwrap_or(app_file_name_without_ext.to_string()),
            &profile,
        )
        .await?;
        // 从配置中心获取配置文件内容并加载到config中
        #[cfg(feature = "config-center")]
        match get_configs().await {
            Ok(config_items) => {
                for item in config_items {
                    config_builder = config_builder
                        .add_source(config::File::from_str(&item.content, item.format));
                }
            }
            Err(e) => {
                warn!("Failed to get configs from config center: {:?}", e);
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
        Err(e) => {
            warn!("micro-svc config not found or deserialize failed: {:?}", e);
            return Ok(());
        }
    };
    if micro_svc_config.svc_name.is_none() {
        micro_svc_config.svc_name = Some(app_name.to_string());
    }
    if micro_svc_config.profile.is_none() {
        micro_svc_config.profile = profile.clone();
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
    app_dir: &PathBuf,
    cfg_file_name_without_ext: &str,
    cfg_file_path: &Option<String>,
    mut config: ConfigBuilder<DefaultState>,
) -> Result<(ConfigBuilder<DefaultState>, Vec<String>)> {
    let mut files = vec![];
    // 如果已指定配置文件路径
    let config = if let Some(cfg_file_path) = cfg_file_path.clone() {
        add_source(config, cfg_file_path.as_str(), None, &mut files)
    } else {
        let temp_path = app_dir
            .join(cfg_file_name_without_ext)
            .to_string_lossy()
            .to_string();
        for ext in ["toml", "json", "json5", "yml", "yaml", "ini", "ron"] {
            config = add_source(config, temp_path.as_str(), Some(ext), &mut files);
        }
        config
    };
    Ok((config, files))
}

fn add_source(
    config: ConfigBuilder<DefaultState>,
    file_path_without_ext: &str,
    ext: Option<&str>,
    files: &mut Vec<String>,
) -> ConfigBuilder<DefaultState> {
    let file_path_string = if let Some(ext) = ext {
        format!("{file_path_without_ext}.{ext}")
    } else {
        file_path_without_ext.to_string()
    };
    let file_path = Path::new(file_path_string.as_str());
    if !file_path.exists() {
        return config;
    }
    files.push(file_path_string.clone());
    let mut file = config::File::with_name(file_path_string.as_str());
    // .json 后缀也用 Json5 格式
    if file_path.ends_with(".json") {
        file = file.format(config::FileFormat::Json5)
    }
    config.add_source(file)
}
