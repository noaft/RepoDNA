use anyhow::{Context, Result, anyhow, bail};
use fastembed::{EmbeddingModel as FastEmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::sync::Mutex;

#[allow(dead_code)]
pub const NOMIC_EMBEDDING_DIMENSIONS: usize = 768;

static NOMIC_EMBEDDER: OnceCell<Mutex<TextEmbedding>> = OnceCell::new();

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingResult {
    pub model: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

pub fn embed_text(text: &str) -> Result<EmbeddingResult> {
    embed_text_with_settings(text, &crate::settings::Settings::from_env().embedding)
}

pub fn embed_text_with_settings(
    text: &str,
    settings: &crate::settings::EmbeddingSettings,
) -> Result<EmbeddingResult> {
    let trimmed = validate_embedding_text(text)?;

    match settings.provider {
        crate::settings::EmbeddingProvider::Nomic => {
            let vector = embed_trimmed_text_with_nomic(trimmed)?;
            Ok(EmbeddingResult {
                model: settings.model.clone(),
                vector,
            })
        }
        crate::settings::EmbeddingProvider::OpenAi => embed_text_with_openai(trimmed, settings),
    }
}

pub fn embed_text_with_nomic(text: &str) -> Result<Vec<f32>> {
    let trimmed = validate_embedding_text(text)?;
    embed_trimmed_text_with_nomic(trimmed)
}

fn validate_embedding_text(text: &str) -> Result<&str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("embedding input text must not be empty");
    }

    Ok(trimmed)
}

fn embed_trimmed_text_with_nomic(trimmed: &str) -> Result<Vec<f32>> {
    let embedder = NOMIC_EMBEDDER.get_or_try_init(|| {
        let model = TextEmbedding::try_new(
            InitOptions::new(FastEmbeddingModel::NomicEmbedTextV15)
                .with_show_download_progress(false),
        )
        .context("failed to initialize nomic embedding model")?;

        Ok::<_, anyhow::Error>(Mutex::new(model))
    })?;

    let mut model = embedder
        .lock()
        .map_err(|_| anyhow!("nomic embedding model lock was poisoned"))?;
    let mut embeddings = model
        .embed(vec![format!("search_document: {trimmed}")], None)
        .context("failed to embed text with nomic embedding model")?;

    embeddings
        .pop()
        .context("nomic embedding model returned no embedding vectors")
}

fn embed_text_with_openai(
    trimmed: &str,
    settings: &crate::settings::EmbeddingSettings,
) -> Result<EmbeddingResult> {
    let api_key = settings
        .openai_api_key
        .as_deref()
        .context("OPENAI_API_KEY is required when REPODNA_EMBEDDING_PROVIDER=openai")?;
    let endpoint = format!(
        "{}/embeddings",
        settings.openai_base_url.trim_end_matches('/')
    );

    let response = reqwest::blocking::Client::new()
        .post(&endpoint)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": settings.model,
            "input": trimmed,
        }))
        .send()
        .with_context(|| format!("failed to request embeddings from {endpoint}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        bail!("OpenAI-compatible embedding request failed with status {status}: {body}");
    }

    let body: OpenAiEmbeddingResponse = response
        .json()
        .context("failed to parse OpenAI-compatible embedding response")?;
    let vector = body
        .data
        .into_iter()
        .next()
        .map(|item| item.embedding)
        .filter(|embedding| !embedding.is_empty())
        .context("OpenAI-compatible embedding response returned no embedding vectors")?;

    Ok(EmbeddingResult {
        model: settings.model.clone(),
        vector,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "downloads/loads the local embedding model"]
    fn embed_text_with_nomic_returns_vector_for_text() -> anyhow::Result<()> {
        let embedding = embed_text_with_nomic("test")?;

        assert_eq!(embedding.len(), NOMIC_EMBEDDING_DIMENSIONS);
        assert!(embedding.iter().any(|value| *value != 0.0));
        println!("embedding dimensions: {}", embedding.len());
        println!("embedding: {embedding:?}");

        Ok(())
    }

    #[test]
    fn embed_text_with_nomic_rejects_empty_text_without_loading_model() {
        let error = embed_text_with_nomic("   ").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("embedding input text must not be empty")
        );
    }

    #[test]
    fn embed_text_returns_model_metadata_from_configured_provider() -> anyhow::Result<()> {
        let settings = crate::settings::Settings::from_pairs([
            ("REPODNA_EMBEDDING_PROVIDER", "openai"),
            ("REPODNA_EMBEDDING_MODEL", "text-embedding-3-small"),
            ("OPENAI_API_KEY", "test-key"),
        ]);

        let result = EmbeddingResult {
            model: settings.embedding.model.clone(),
            vector: vec![0.1, 0.2],
        };

        assert_eq!(result.model, "text-embedding-3-small");
        assert_eq!(result.vector, vec![0.1, 0.2]);

        Ok(())
    }
}
