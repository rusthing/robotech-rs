use crate::cfg::cfg_error::CfgError;
use crate::env::{AppEnv, EnvError, APP_ENV};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use std::path::Path;

pub fn build_config<'a, T: serde::Deserialize<'a>>(
    env_var_prefix: &str,
    cfg_file_name_without_ext: Option<&str>,
    cfg_file_path: Option<String>,
) -> Result<T, CfgError> {
    // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
    let mut config = Config::builder();

    // 如果已指定配置文件路径
    config = if let Some(cfg_file_path) = cfg_file_path.clone() {
        add_source(config, cfg_file_path.as_str(), None)
    } else {
        let AppEnv {
            app_dir,
            app_file_name_without_ext,
            ..
        } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
        let temp_path = app_dir
            .join(
                if let Some(cfg_file_name_without_ext) = cfg_file_name_without_ext {
                    cfg_file_name_without_ext
                } else {
                    app_file_name_without_ext
                },
            )
            .to_string_lossy()
            .to_string();
        config = add_source(config, temp_path.as_str(), Some("toml"));
        config = add_source(config, temp_path.as_str(), Some("yml"));
        config = add_source(config, temp_path.as_str(), Some("json"));
        config = add_source(config, temp_path.as_str(), Some("ini"));
        config = add_source(config, temp_path.as_str(), Some("ron"));
        config
    };

    // 后续添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in cfg from the environment (with a prefix of XXX)
        // E.g. `XXX_DEBUG=true ./target/app` would set the `debug` to `true`
        .add_source(config::Environment::with_prefix(env_var_prefix))
        .build()
        .map_err(CfgError::Build)?;

    Ok(config.try_deserialize().map_err(CfgError::Deserialize)?)
}

fn add_source(
    config: ConfigBuilder<DefaultState>,
    file_path_without_ext: &str,
    ext: Option<&str>,
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
    let file = config::File::with_name(file_path_string.as_str());
    config.add_source(file)
}
