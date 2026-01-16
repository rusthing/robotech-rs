use crate::env::ENV;
use config::builder::DefaultState;
use config::{Config, ConfigBuilder, ConfigError};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

pub fn parse_config<'de, T: Deserialize<'de>>(
    path: Option<String>,
) -> Result<(T, Vec<String>), ConfigError> {
    let config = Config::builder();
    let config = if let Some(path) = path {
        // 如果已指定配置文件路径
        let config_file_path = config::File::with_name(path.as_str());
        (config.add_source(config_file_path), vec![path])
    } else {
        // 如果未指定配置文件路径
        let env = ENV.get().unwrap();
        let path = env
            .app_dir
            .join(env.app_file_name.as_str())
            .to_string_lossy()
            .to_string();

        // 创建通道接收事件
        let (tx, rx) = std::sync::mpsc::channel();

        // 创建推荐的 watcher
        let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;

        // 监听文件（非递归）
        watcher.watch("config.toml", RecursiveMode::NonRecursive)?;

        // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
        let config_file_path = format!("{}.toml", path).as_str();
        // 判断文件是否存在
        add_source(&config, format!("{}.toml", path).as_str());
        add_source(&config, format!("{}.yml", path).as_str());
        add_source(&config, format!("{}.json", path).as_str());
        add_source(&config, format!("{}.ini", path).as_str());
        add_source(&config, format!("{}.ron", path).as_str());
    };
    // 后续添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in config from the environment (with a prefix of APP)
        // E.g. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .unwrap();

    config.try_deserialize::<T>().unwrap()
}

fn add_source(config: &ConfigBuilder<DefaultState>, config_file_path: &str) {
    if !std::path::Path::new(config_file_path).exists() {
        let _ = config.add_source(config::File::with_name(config_file_path).required(false));
    }
}
