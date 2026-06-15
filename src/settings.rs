use std::{env, path::PathBuf};

pub const ENV_DB_PATH: &str = "REPODNA_DB_PATH";
pub const ENV_HOME: &str = "REPODNA_HOME";
pub const ENV_EMBEDDING_PROVIDER: &str = "REPODNA_EMBEDDING_PROVIDER";
pub const ENV_EMBEDDING_MODEL: &str = "REPODNA_EMBEDDING_MODEL";
pub const ENV_OPENAI_API_KEY: &str = "OPENAI_API_KEY";
pub const ENV_OPENAI_BASE_URL: &str = "OPENAI_BASE_URL";
pub const NOMIC_EMBEDDING_MODEL: &str = "nomic-ai/nomic-embed-text-v1.5";
pub const DEFAULT_OPENAI_EMBEDDING_MODEL: &str = "text-embedding-3-small";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const APP_DIR_NAME: &str = "RepoDNA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbeddingProvider {
    Nomic,
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct EmbeddingSettings {
    pub provider: EmbeddingProvider,
    pub model: String,
    pub openai_api_key: Option<String>,
    pub openai_base_url: String,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub db_path: Option<PathBuf>,
    pub storage_home: PathBuf,
    pub storage_home_from_env: bool,
    pub embedding: EmbeddingSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            db_path: None,
            storage_home: default_storage_home(),
            storage_home_from_env: false,
            embedding: EmbeddingSettings::default(),
        }
    }
}

impl Settings {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| env::var(key).ok())
    }

    #[cfg(test)]
    pub fn from_pairs<const N: usize>(pairs: [(&str, &str); N]) -> Self {
        Self::from_lookup(|key| {
            pairs
                .iter()
                .find(|(pair_key, _)| *pair_key == key)
                .map(|(_, value)| (*value).to_string())
        })
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let mut settings = Self::default();

        if let Some(path) = lookup_path(&mut lookup, ENV_DB_PATH) {
            settings.db_path = Some(path);
        }

        if let Some(path) = lookup_path(&mut lookup, ENV_HOME) {
            settings.storage_home = path;
            settings.storage_home_from_env = true;
        }

        settings.embedding = EmbeddingSettings::from_lookup(lookup);
        settings
    }
}

impl Default for EmbeddingSettings {
    fn default() -> Self {
        Self {
            provider: EmbeddingProvider::Nomic,
            model: NOMIC_EMBEDDING_MODEL.to_string(),
            openai_api_key: None,
            openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
        }
    }
}

impl EmbeddingSettings {
    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let provider = match lookup_trimmed(&mut lookup, ENV_EMBEDDING_PROVIDER)
            .unwrap_or_else(|| "nomic".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "openai" => EmbeddingProvider::OpenAi,
            _ => EmbeddingProvider::Nomic,
        };

        let model = match provider {
            EmbeddingProvider::Nomic => NOMIC_EMBEDDING_MODEL.to_string(),
            EmbeddingProvider::OpenAi => lookup_trimmed(&mut lookup, ENV_EMBEDDING_MODEL)
                .unwrap_or_else(|| DEFAULT_OPENAI_EMBEDDING_MODEL.to_string()),
        };

        Self {
            provider,
            model,
            openai_api_key: lookup_trimmed(&mut lookup, ENV_OPENAI_API_KEY),
            openai_base_url: lookup_trimmed(&mut lookup, ENV_OPENAI_BASE_URL)
                .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string()),
        }
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

fn lookup_path(lookup: &mut impl FnMut(&str) -> Option<String>, key: &str) -> Option<PathBuf> {
    lookup_trimmed(lookup, key).map(PathBuf::from)
}

fn lookup_trimmed(lookup: &mut impl FnMut(&str) -> Option<String>, key: &str) -> Option<String> {
    lookup(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_local_nomic_embeddings() {
        let settings = Settings::from_pairs([]);

        assert_eq!(settings.embedding.provider, EmbeddingProvider::Nomic);
        assert_eq!(settings.embedding.model, "nomic-ai/nomic-embed-text-v1.5");
        assert_eq!(
            settings.embedding.openai_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(settings.embedding.openai_api_key, None);
    }

    #[test]
    fn settings_parse_openai_compatible_embedding_env() {
        let settings = Settings::from_pairs([
            ("REPODNA_EMBEDDING_PROVIDER", "openai"),
            ("REPODNA_EMBEDDING_MODEL", "nomic-embed-text"),
            ("OPENAI_BASE_URL", "http://localhost:11434/v1"),
            ("OPENAI_API_KEY", "local-key"),
        ]);

        assert_eq!(settings.embedding.provider, EmbeddingProvider::OpenAi);
        assert_eq!(settings.embedding.model, "nomic-embed-text");
        assert_eq!(
            settings.embedding.openai_base_url,
            "http://localhost:11434/v1"
        );
        assert_eq!(
            settings.embedding.openai_api_key.as_deref(),
            Some("local-key")
        );
    }

    #[test]
    fn settings_keep_nomic_model_fixed_for_local_provider() {
        let settings = Settings::from_pairs([
            ("REPODNA_EMBEDDING_PROVIDER", "nomic"),
            ("REPODNA_EMBEDDING_MODEL", "text-embedding-3-small"),
        ]);

        assert_eq!(settings.embedding.provider, EmbeddingProvider::Nomic);
        assert_eq!(settings.embedding.model, NOMIC_EMBEDDING_MODEL);
    }
}
