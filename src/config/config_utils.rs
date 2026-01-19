use crate::config::ConfigError;
use crate::env::ENV;
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::Path;

pub fn parse_config<'de, T: Deserialize<'de>>(
    path: Option<String>,
) -> Result<
    (
        T,
        std::sync::mpsc::Receiver<notify::Result<notify_types::event::Event>>,
    ),
    ConfigError,
> {
    // 创建通道接收事件
    let (tx, rx) = std::sync::mpsc::channel();
    // watcher监听文件更新
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;

    let config = Config::builder();
    let config = if let Some(path) = path {
        // 如果已指定配置文件路径
        let config_file_path = config::File::with_name(path.as_str());
        config.add_source(config_file_path)
    } else {
        // 如果未指定配置文件路径
        let env = ENV.get().unwrap();
        let app_file_path = env.app_dir.join(env.app_file_name.as_str());

        // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
        add_source(&config, app_file_path.join(".toml").as_path(), &mut watcher);
        add_source(&config, app_file_path.join(".yml").as_path(), &mut watcher);
        add_source(&config, app_file_path.join(".json").as_path(), &mut watcher);
        add_source(&config, app_file_path.join(".ini").as_path(), &mut watcher);
        add_source(&config, app_file_path.join(".ron").as_path(), &mut watcher);
        config
    };

    // 后续添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in config from the environment (with a prefix of APP)
        // E.g. `APP_DEBUG=1 ./target/app` would set the `debug` key
        .add_source(config::Environment::with_prefix("APP"))
        .build()?;

    Ok((config.try_deserialize::<T>()?, rx))
}

fn add_source(
    config: &ConfigBuilder<DefaultState>,
    config_file_path: &Path,
    watcher: &mut RecommendedWatcher,
) {
    // 判断文件是否存在
    if !Path::new(config_file_path).exists() {
        // 添加源
        let _ = config.clone().add_source(config::File::with_name(
            config_file_path.to_string_lossy().to_string().as_str(),
        ));

        // 监听文件（非递归）
        watcher
            .watch(&config_file_path, RecursiveMode::NonRecursive)
            .ok();
    }
}
