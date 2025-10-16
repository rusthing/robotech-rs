use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct DbSettings {
    #[serde(default = "url_default")]
    pub url: String,
}

impl Default for DbSettings {
    fn default() -> Self {
        db_default()
    }
}

fn url_default() -> String {
    "".to_string()
}

fn db_default() -> DbSettings {
    DbSettings { url: url_default() }
}
