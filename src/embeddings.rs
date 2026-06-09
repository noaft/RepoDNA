use anyhow::{Context, Result, anyhow, bail};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use std::sync::Mutex;

#[allow(dead_code)]
pub const NOMIC_EMBEDDING_DIMENSIONS: usize = 768;

static NOMIC_EMBEDDER: OnceCell<Mutex<TextEmbedding>> = OnceCell::new();

pub fn embed_text_with_nomic(text: &str) -> Result<Vec<f32>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("embedding input text must not be empty");
    }

    let embedder = NOMIC_EMBEDDER.get_or_try_init(|| {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::NomicEmbedTextV15).with_show_download_progress(false),
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
}
