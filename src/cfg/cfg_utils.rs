use crate::cfg::cfg_error::CfgError;
use crate::env::{AppEnv, EnvError, APP_ENV};
use config::Config;

pub fn build_config<'a, T: serde::Deserialize<'a>>(
    env_var_prefix: &str,
    path: Option<String>,
) -> Result<T, CfgError> {
    let mut config = Config::builder();
    let AppEnv { app_dir, .. } = APP_ENV.get().ok_or(EnvError::GetAppEnv())?;
    let temp_path = app_dir.join("log").to_string_lossy().to_string();

    // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
    config = config
        .add_source(config::File::with_name(format!("{}.toml", temp_path).as_str()).required(false))
        .add_source(config::File::with_name(format!("{}.yml", temp_path).as_str()).required(false))
        .add_source(config::File::with_name(format!("{}.json", temp_path).as_str()).required(false))
        .add_source(config::File::with_name(format!("{}.ini", temp_path).as_str()).required(false))
        .add_source(config::File::with_name(format!("{}.ron", temp_path).as_str()).required(false));

    if let Some(temp_path) = path.clone() {
        // 如果已指定配置文件路径
        let temp_path = config::File::with_name(temp_path.as_str());
        config = config.add_source(temp_path);
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
