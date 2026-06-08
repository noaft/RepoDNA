use std::{env, path::PathBuf};

pub const ENV_DB_PATH: &str = "REPODNA_DB_PATH";
pub const ENV_HOME: &str = "REPODNA_HOME";
const APP_DIR_NAME: &str = "RepoDNA";

#[derive(Debug, Clone)]
pub struct Settings {
    pub db_path: Option<PathBuf>,
    pub storage_home: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            db_path: None,
            storage_home: default_storage_home(),
        }
    }
}

impl Settings {
    pub fn from_env() -> Self {
        let mut settings = Self::default();

        if let Some(path) = env_path(ENV_DB_PATH) {
            settings.db_path = Some(path);
        }

        if let Some(path) = env_path(ENV_HOME) {
            settings.storage_home = path;
        }

        settings
    }
}

fn default_storage_home() -> PathBuf {
    if let Some(local_app_data) = env_path("LOCALAPPDATA") {
        return local_app_data.join(APP_DIR_NAME);
    }

    if let Some(home) = env_path("HOME") {
        return home.join(".repodna");
    }

    PathBuf::from(".repodna")
}

fn env_path(key: &str) -> Option<PathBuf> {
    let raw = env::var_os(key)?;
    let path = PathBuf::from(raw);
    (!path.as_os_str().is_empty()).then_some(path)
}
