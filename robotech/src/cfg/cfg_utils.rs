use crate::cfg::base_config::BaseConfig;
use crate::cfg::cfg_error::CfgError;
use config::builder::DefaultState;
use config::{Config, ConfigBuilder, Map, Value, ValueKind};
use std::path::{Path, PathBuf};

pub type Result<T> = core::result::Result<T, CfgError>;

pub async fn build_cfg(
    app_dir: &PathBuf,
    env_var_prefix: &str,
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

    // // 从配置中心获取配置文件内容并加载到config中
    // #[cfg(any(feature = "config-center", feature = "registry-center"))]
    // if let Some((content, format)) = init_hub_client(
    //     app_dir,
    //     app_file_name_without_ext,
    //     cfg_file_name_without_ext,
    //     &cfg_file_path,
    //     &profile,
    //     cfg_changed_tx,
    // )
    // .await?
    // {
    //     config = config.add_source(config::File::from_str(&content, format));
    // }

    // 加载配置文件
    let (config, files) =
        add_cfg_files(app_dir, cfg_file_name_without_ext, &cfg_file_path, config)?;

    // 加载profile对应的配置文件
    let (config, files) = if let Some(profile) = base_config.clone().profile {
        add_cfg_files(
            app_dir,
            format!("{}-{}", cfg_file_name_without_ext, profile).as_str(),
            &cfg_file_path,
            config,
        )?
    } else {
        (config, files)
    };

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

/// Compare two `Config` values using the crate's own `Value` tree.
/// Returns a map of changed top-level key -> new value.
/// An empty map means the two configs are identical.
/// Removed keys are represented with `ValueKind::Nil`.
pub fn diff_config(old: &Config, new: &Config) -> Map<String, Value> {
    let old_table = match &old.cache.kind {
        ValueKind::Table(table) => table,
        _ => return Map::new(),
    };
    let new_table = match &new.cache.kind {
        ValueKind::Table(table) => table,
        _ => return Map::new(),
    };

    let mut changed = Map::new();
    for (key, new_val) in new_table {
        match old_table.get(key) {
            Some(old_val) if old_val == new_val => {}
            _ => {
                changed.insert(key.clone(), new_val.clone());
            }
        }
    }
    for key in old_table.keys() {
        if !new_table.contains_key(key) && !changed.contains_key(key) {
            changed.insert(key.clone(), Value::new(None, ValueKind::Nil));
        }
    }
    changed
}

// #[cfg(any(feature = "config-center", feature = "registry-center"))]
// async fn init_hub_client(
//     app_dir: &PathBuf,
//     app_name: &str,
//     cfg_file_name_without_ext: &str,
//     cfg_file_path: &Option<String>,
//     profile: &Option<String>,
//     cfg_changed_tx: watch::Sender<()>,
// ) -> Result<Option<(String, config::FileFormat)>, CfgError> {
//     let config = Config::builder();
//     let (config, ..) = add_cfg_files(app_dir, cfg_file_name_without_ext, cfg_file_path, config)?;
//     let config: MicroSvcConfig = config
//         .build()
//         .map_err(CfgError::Build)?
//         .try_deserialize()
//         .map_err(CfgError::Deserialize)?;
//
//     let (config_center_client, registry_center_client) =
//         HubClient::init(app_name, profile, config).await?;
//     let config_item = match config_center_client {
//         Some(client) => Some(
//             client
//                 .fetch()
//                 .await
//                 .map_err(|e| CfgError::Init(e.to_string()))?,
//         ),
//         None => None,
//     };
//     Ok(config_item.map(|item| (item.content, item.format)))
// }

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