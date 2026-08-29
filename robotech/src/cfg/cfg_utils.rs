use crate::cfg::base_config::BaseConfig;
use crate::cfg::cfg_error::CfgError;
#[cfg(any(feature = "config-center", feature = "registry-center"))]
use crate::micro_svc::{get_hub_client, setup_hub_client, MicroSvcConfig, MICRO_SVC_CONFIG_KEY};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use std::path::{Path, PathBuf};
use tracing::warn;

pub type Result<T> = core::result::Result<T, CfgError>;

pub async fn build_cfg(
    app_dir: &PathBuf,
    env_var_prefix: &str,
    app_file_name_without_ext: Option<&str>,
    cfg_file_name_without_ext: &str,
    cfg_file_path: Option<String>,
) -> Result<(BaseConfig, Config, Vec<String>)> {
    // 先加载基础配置文件获取profile，后续根据profile加载对应的配置文件
    let config = Config::builder();
    let (config, ..) = add_cfg_files(app_dir, cfg_file_name_without_ext, &cfg_file_path, config)?;
    let base_config: BaseConfig = config
        .build()
        .map_err(CfgError::Build)?
        .try_deserialize()
        .map_err(CfgError::Deserialize)?;

    let config = Config::builder();

    // 加载配置文件
    let (config, mut files) =
        add_cfg_files(app_dir, cfg_file_name_without_ext, &cfg_file_path, config)?;

    // 加载profile对应的配置文件
    let (mut config, files) = if let Some(profile) = &base_config.profile {
        let (config, profile_files) = add_cfg_files(
            app_dir,
            format!("{}-{}", cfg_file_name_without_ext, profile).as_str(),
            &cfg_file_path,
            config,
        )?;
        files.extend(profile_files);
        (config, files)
    } else {
        (config, files)
    };

    // 初始化配置中心和注册中心的客户端
    // 如果传入app_file_name_without_ext为None，说明是构建log配置，不需要通过配置中心初始化配置
    #[cfg(any(feature = "config-center", feature = "registry-center"))]
    if let Some(app_name) = app_file_name_without_ext {
        // 初始化配置中心和注册中心的客户端
        #[cfg(any(feature = "config-center", feature = "registry-center"))]
        let has_hub_client =
            init_hub_client(config.clone(), app_name, &base_config.profile).await?;
        // 从配置中心获取配置文件内容并加载到config中
        #[cfg(feature = "config-center")]
        if has_hub_client && let Some(config_item) = get_hub_client()?.get_config().await? {
            config = config.add_source(config::File::from_str(
                &config_item.content,
                config_item.format,
            ));
        }
    }

    // 添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in app from the environment (with a prefix of XXX)
        // E.g. `XXX_DEBUG=true ./target/app` would set the `debug` to `true`
        .add_source(config::Environment::with_prefix(env_var_prefix))
        .build()
        .map_err(CfgError::Build)?;

    Ok((base_config, config, files))
}

pub async fn deserialize_config<'a, T>(config: Config) -> Result<T>
where
    T: serde::Deserialize<'a>,
{
    config.try_deserialize().map_err(CfgError::Deserialize)
}

#[cfg(any(feature = "config-center", feature = "registry-center"))]
async fn init_hub_client(
    config: ConfigBuilder<DefaultState>,
    app_name: &str,
    profile: &Option<String>,
) -> Result<bool> {
    // 如果 micro-svc 没配置，直接返回 None，跳过 hub client 初始化
    let mut micro_svc_config: MicroSvcConfig = match config
        .build()
        .map_err(CfgError::Build)?
        .get(MICRO_SVC_CONFIG_KEY)
    {
        Ok(value) => value,
        Err(e) => {
            warn!("micro-svc config not found or deserialize failed: {:?}", e);
            return Ok(false);
        }
    };
    if micro_svc_config.svc_name.is_none() {
        micro_svc_config.svc_name = Some(app_name.to_string());
    }
    if micro_svc_config.profile.is_none() {
        micro_svc_config.profile = profile.clone();
    }
    if get_hub_client().is_err() {
        setup_hub_client(micro_svc_config).await?;
    }
    Ok(true)
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
