use crate::cfg::cfg_error::CfgError;
use crate::env::{APP_ENV, AppEnv, EnvError};
use config::builder::DefaultState;
use config::{Config, ConfigBuilder};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

pub fn build_config<'a, T: serde::Deserialize<'a>>(
    env_var_prefix: &str,
    cfg_file_name_without_ext: Option<&str>,
    cfg_file_path: Option<String>,
) -> Result<(T, Vec<String>), CfgError> {
    // Add in `./xxx.toml`, `./xxx.yml`, `./xxx.json`, `./xxx.ini`, `./xxx.ron`
    let mut config = Config::builder();

    let mut files = vec![];
    // 如果已指定配置文件路径
    config = if let Some(cfg_file_path) = cfg_file_path.clone() {
        add_source(config, cfg_file_path.as_str(), None, &mut files)
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
        config = add_source(config, temp_path.as_str(), Some("toml"), &mut files);
        config = add_source(config, temp_path.as_str(), Some("yml"), &mut files);
        config = add_source(config, temp_path.as_str(), Some("json"), &mut files);
        config = add_source(config, temp_path.as_str(), Some("ini"), &mut files);
        config = add_source(config, temp_path.as_str(), Some("ron"), &mut files);
        config
    };

    // 后续添加环境变量，以覆盖配置文件中的设置
    let config = config
        // Add in cfg from the environment (with a prefix of XXX)
        // E.g. `XXX_DEBUG=true ./target/app` would set the `debug` to `true`
        .add_source(config::Environment::with_prefix(env_var_prefix))
        .build()
        .map_err(CfgError::Build)?;

    Ok((
        config.try_deserialize().map_err(CfgError::Deserialize)?,
        files,
    ))
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
    let file = config::File::with_name(file_path_string.as_str());
    config.add_source(file)
}

pub fn watch_config_file(
    files: Vec<String>,
    // sender: mpsc::Sender<()>,
) -> Result<
    (
        Debouncer<RecommendedWatcher>,
        mpsc::Receiver<DebounceEventResult>,
    ),
    notify::Error,
> {
    // let mut watcher = recommended_watcher(move |res: Result<Event, notify::Error>| {
    //     if let Ok(event) = res {
    //         // 只关心文件修改事件
    //         if matches!(event.kind, EventKind::Modify(_)) {
    //             sender.send(()).ok();
    //         }
    //     }
    // })?;
    //
    // for file in files {
    //     // 监控配置文件
    //     watcher.watch(Path::new(&file), RecursiveMode::NonRecursive)?;
    // }

    let (sender, receiver) = mpsc::channel();

    let mut debouncer = new_debouncer(
        Duration::from_millis(500), // 防抖延迟时间
        sender,
    )?;

    // 开始监控
    for file in files {
        debouncer
            .watcher()
            .watch(Path::new(&file), RecursiveMode::NonRecursive)?;
    }

    Ok((debouncer, receiver))
}
