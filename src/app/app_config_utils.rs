use crate::app::AppConfigError;
use crate::env::{Env, EnvError, ENV};
use config::Config;

pub fn build_app_config<'a, T: serde::Deserialize<'a>>(
    path: Option<String>,
) -> Result<T, AppConfigError> {
    let mut config = Config::builder();
    if path.is_none() {
        let Env { app_file_path, .. } = ENV.get().ok_or(EnvError::GetEnv())?;
        let temp_path = app_file_path.to_string_lossy().to_string();

        // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
        config = config
            .add_source(
                config::File::with_name(format!("{}.toml", temp_path).as_str()).required(false),
            )
            .add_source(
                config::File::with_name(format!("{}.yml", temp_path).as_str()).required(false),
            )
            .add_source(
                config::File::with_name(format!("{}.json", temp_path).as_str()).required(false),
            )
            .add_source(
                config::File::with_name(format!("{}.ini", temp_path).as_str()).required(false),
            )
            .add_source(
                config::File::with_name(format!("{}.ron", temp_path).as_str()).required(false),
            );
    }

    if let Some(temp_path) = path.clone() {
        // 如果已指定配置文件路径
        let temp_path = config::File::with_name(temp_path.as_str());
        config = config.add_source(temp_path);
    };

    // 后续添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in app from the environment (with a prefix of APP)
        // E.g. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .map_err(AppConfigError::Build)?;

    Ok(config
        .try_deserialize()
        .map_err(AppConfigError::Deserialize)?)
}
