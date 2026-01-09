use crate::env::ENV;
use config::Config;
use serde::Deserialize;

pub fn get_config<'de, T: Deserialize<'de>>(path: Option<String>) -> T {
    let config = Config::builder();
    let config = if path.is_some() {
        let path = path.unwrap();
        // 判断文件是否存在
        if !std::path::Path::new(&path).exists() {
            panic!("The specified configuration file does not exist");
        }
        // 如果已指定配置文件路径
        config.add_source(config::File::with_name(path.as_str()).required(false))
    } else {
        // 如果未指定配置文件路径
        let env = ENV.get().unwrap();
        let path = env
            .app_dir
            .join(env.app_file_name.as_str())
            .to_string_lossy()
            .to_string();

        // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
        config
            .add_source(config::File::with_name(format!("{}.toml", path).as_str()).required(false))
            .add_source(config::File::with_name(format!("{}.yml", path).as_str()).required(false))
            .add_source(config::File::with_name(format!("{}.json", path).as_str()).required(false))
            .add_source(config::File::with_name(format!("{}.ini", path).as_str()).required(false))
            .add_source(config::File::with_name(format!("{}.ron", path).as_str()).required(false))
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
