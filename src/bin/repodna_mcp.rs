use anyhow::{Context, Result, bail};
#[path = "../embeddings.rs"]
mod embeddings;
use git2::Repository;
#[path = "../repodna_paths.rs"]
mod repodna_paths;
#[path = "../settings.rs"]
mod settings;
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::ServerInfo,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const NOMIC_EMBEDDING_MODEL: &str = "nomic-ai/nomic-embed-text-v1.5";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchFunctionsParams {
    /// Exact-ish function lookup only: pass one function name, function id, Rust symbol path, or file path hint.
    /// Do not pass natural language, behavior descriptions, or many unrelated words; use search_function_contexts for that.
    query: String,
    /// Maximum number of active function nodes to return. Defaults to 20 and is clamped to 1..=100.
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchFunctionContextsParams {
    /// Natural-language or behavior-oriented query for saved function summaries, such as "starts MCP server" or "builds repository graph".
    query: String,
    /// Maximum number of active function contexts to return. Defaults to 20 and is clamped to 1..=100.
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddFunctionContextParams {
    /// Exact function id returned by search_functions.
    function_id: String,
    /// Concise high-level description of what the function is for.
    summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct FunctionNodeResult {
    summary: String,
    id: String,
    r#type: String,
    name: String,
    metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SearchFunctionsResponse {
    results: Vec<FunctionNodeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SearchFunctionContextsResponse {
    results: Vec<FunctionNodeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddFunctionContextResponse {
    function_id: String,
    summary: String,
}

#[derive(Clone)]
struct RepoDnaMcp {
    db_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RepoDnaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "RepoDNA is persistent repository memory. Use a memory-first workflow: before reading files broadly to understand repository code, call search_function_contexts for natural-language, behavioral, or semantic questions. Only call search_functions when you already have one concrete function name, function id, Rust symbol path, or file path hint; do not pass long free-text context into search_functions. If a matching function is found with a useful summary, use that saved context first. If the function exists but its summary is empty or too generic, inspect the source code yourself, then call add_function_context with the exact function_id and a concise high-level summary so future sessions do not rediscover it. If RepoDNA returns no relevant result, fall back to normal code search and reading.".to_string(),
            ),
            ..Default::default()
        }
    }
}

#[tool_router(router = tool_router)]
impl RepoDnaMcp {
    fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Exact-ish function lookup in RepoDNA's graph. Use ONLY when you already know one concrete function name, function id, Rust symbol path, or file path hint. Do NOT pass natural language, behavior descriptions, or combined keywords like 'run main build graph CLI entrypoint'; use search_function_contexts for that. Parameter query should be a short single lookup hint, e.g. 'serve_graph_api', 'src/api/mod.rs', or an exact function_id. Returns active function nodes with names, ids, summaries, and metadata."
    )]
    async fn search_functions(
        &self,
        Parameters(SearchFunctionsParams { query, limit }): Parameters<SearchFunctionsParams>,
    ) -> Result<Json<SearchFunctionsResponse>, String> {
        search_function_nodes(&self.db_path, &query, limit.unwrap_or(20))
            .map(|results| Json(SearchFunctionsResponse { results }))
            .map_err(|err| err.to_string())
    }

    #[tool(
        description = "Search saved function context by summary. Use this FIRST for natural-language, behavioral, or semantic questions about what code does, such as 'CLI entrypoint for build/update' or 'starts MCP stdio server'. Parameter query may be a phrase or sentence. This searches only active functions with non-empty summaries, ranks matches by summary phrase and term overlap, and returns node-shaped JSON results. If this returns no useful result, use search_functions only with a concrete function name/id/file hint, then read source/add_function_context as needed."
    )]
    async fn search_function_contexts(
        &self,
        Parameters(SearchFunctionContextsParams { query, limit }): Parameters<
            SearchFunctionContextsParams,
        >,
    ) -> Result<Json<SearchFunctionContextsResponse>, String> {
        search_function_contexts(&self.db_path, &query, limit.unwrap_or(20))
            .map(|results| Json(SearchFunctionContextsResponse { results }))
            .map_err(|err| err.to_string())
    }

    #[tool(
        description = "Add or replace durable context for a function node in RepoDNA's graph. Use this after search_functions finds a function but the summary is empty, stale, or too generic, and you have inspected the source code enough to summarize what the function is for. Save concise, high-level context that future tools can trust before rediscovering the code. Requires an exact function_id from search_functions and a summary."
    )]
    async fn add_function_context(
        &self,
        Parameters(AddFunctionContextParams {
            function_id,
            summary,
        }): Parameters<AddFunctionContextParams>,
    ) -> Result<Json<AddFunctionContextResponse>, String> {
        add_function_context(&self.db_path, &function_id, &summary)
            .map(Json)
            .map_err(|err| err.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let repo_path = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let db_path = if let Some(path) = settings::Settings::from_env().db_path {
        path
    } else {
        let repo = Repository::discover(&repo_path)
            .with_context(|| format!("failed to discover repository from '{}'", repo_path))?;
        repodna_paths::validate_storage_configuration(&repo)?;
        repodna_paths::resolve_graph_db_path(&repo)
    };

    let service = RepoDnaMcp::new(db_path).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn search_function_nodes(
    db_path: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<FunctionNodeResult>> {
    let conn = open_existing_graph_db(db_path)?;
    let trimmed = query.trim();
    validate_function_lookup_query(trimmed)?;
    let search = format!("%{}%", trimmed);
    let safe_limit = limit.clamp(1, 100) as i64;

    let mut stmt = conn.prepare(
        "SELECT
            COALESCE(summary, ''),
            id,
            type,
            name,
            metadata
         FROM nodes
         WHERE type = 'Function'
           AND COALESCE(CAST(json_extract(metadata, '$.is_active') AS INTEGER), 1) = 1
           AND (
               ?1 = ''
               OR id LIKE ?2
               OR name LIKE ?2
               OR COALESCE(json_extract(metadata, '$.file'), '') LIKE ?2
           )
         ORDER BY name ASC, id ASC
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(params![trimmed, search, safe_limit], |row| {
        Ok(FunctionNodeResult {
            summary: row.get(0)?,
            id: row.get(1)?,
            r#type: row.get(2)?,
            name: row.get(3)?,
            metadata: row.get(4)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok(items)
}

fn validate_function_lookup_query(query: &str) -> Result<()> {
    if query.is_empty() {
        bail!(
            "search_functions query must be one concrete function name, function_id, Rust symbol path, or file path hint. Use search_function_contexts for broad context search."
        );
    }

    let words = query
        .split_whitespace()
        .filter(|word| !word.trim().is_empty())
        .count();
    if words > 3 {
        bail!(
            "search_functions is for exact function lookup, not natural-language context. Use search_function_contexts for behavior or semantic queries."
        );
    }

    Ok(())
}

fn search_function_contexts(
    db_path: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<FunctionNodeResult>> {
    let conn = open_existing_graph_db(db_path)?;
    let terms = query_terms(query);
    let safe_limit = limit.clamp(1, 100);

    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT
            COALESCE(summary, ''),
            id,
            type,
            name,
            metadata
         FROM nodes
         WHERE type = 'Function'
           AND COALESCE(CAST(json_extract(metadata, '$.is_active') AS INTEGER), 1) = 1
           AND TRIM(COALESCE(summary, '')) <> ''",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(FunctionNodeResult {
            summary: row.get(0)?,
            id: row.get(1)?,
            r#type: row.get(2)?,
            name: row.get(3)?,
            metadata: row.get(4)?,
        })
    })?;

    let phrase = query.trim().to_ascii_lowercase();
    let mut scored = Vec::new();
    for row in rows {
        let item = row?;
        let summary = item.summary.to_ascii_lowercase();
        let matched_terms = terms.iter().filter(|term| summary.contains(*term)).count();
        if matched_terms == 0 {
            continue;
        }

        let phrase_bonus = usize::from(!phrase.is_empty() && summary.contains(&phrase));
        let score = matched_terms + phrase_bonus * terms.len();
        scored.push((score, item.name.clone(), item.id.clone(), item));
    }

    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    Ok(scored
        .into_iter()
        .take(safe_limit)
        .map(|(_, _, _, item)| item)
        .collect())
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| term.len() > 1)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn add_function_context(
    db_path: &Path,
    function_id: &str,
    summary: &str,
) -> Result<AddFunctionContextResponse> {
    add_function_context_with_embedder(
        db_path,
        function_id,
        summary,
        embeddings::embed_text_with_nomic,
    )
}

fn add_function_context_with_embedder(
    db_path: &Path,
    function_id: &str,
    summary: &str,
    embedder: impl Fn(&str) -> Result<Vec<f32>>,
) -> Result<AddFunctionContextResponse> {
    let mut conn = open_existing_graph_db(db_path)?;
    let trimmed_id = function_id.trim();
    let trimmed_summary = summary.trim();

    if trimmed_id.is_empty() {
        bail!("function_id must not be empty");
    }
    if trimmed_summary.is_empty() {
        bail!("summary must not be empty");
    }

    ensure_function_summary_embedding_schema(&conn)?;
    let active_function_exists: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM nodes
            WHERE id = ?1
              AND type = 'Function'
              AND COALESCE(CAST(json_extract(metadata, '$.is_active') AS INTEGER), 1) = 1
        )",
        [trimmed_id],
        |row| row.get(0),
    )?;

    if !active_function_exists {
        bail!(
            "active function node not found for function_id '{}'",
            trimmed_id
        );
    }

    let embedding = embedder(trimmed_summary).context("failed to embed function summary")?;
    if embedding.is_empty() {
        bail!("embedding model returned an empty vector");
    }

    let dimensions = embedding.len();
    let embedding_blob = encode_embedding_blob(&embedding);
    let summary_hash = stable_summary_hash(trimmed_summary);

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE nodes
         SET summary = ?2
         WHERE id = ?1
           AND type = 'Function'
           AND COALESCE(CAST(json_extract(metadata, '$.is_active') AS INTEGER), 1) = 1",
        params![trimmed_id, trimmed_summary],
    )?;
    tx.execute(
        "INSERT INTO function_summary_embeddings (
            function_id,
            model,
            dimensions,
            summary_hash,
            embedding,
            updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(function_id) DO UPDATE SET
            model = excluded.model,
            dimensions = excluded.dimensions,
            summary_hash = excluded.summary_hash,
            embedding = excluded.embedding,
            updated_at = excluded.updated_at",
        params![
            trimmed_id,
            NOMIC_EMBEDDING_MODEL,
            dimensions as i64,
            summary_hash,
            embedding_blob
        ],
    )?;
    tx.commit()?;

    Ok(AddFunctionContextResponse {
        function_id: trimmed_id.to_string(),
        summary: trimmed_summary.to_string(),
    })
}

fn ensure_function_summary_embedding_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS function_summary_embeddings (
            function_id TEXT PRIMARY KEY,
            model TEXT NOT NULL,
            dimensions INTEGER NOT NULL,
            summary_hash TEXT NOT NULL,
            embedding BLOB NOT NULL,
            updated_at TEXT NOT NULL
        )",
        [],
    )?;

    Ok(())
}

fn encode_embedding_blob(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn stable_summary_hash(summary: &str) -> String {
    format!("{:016x}", fnv1a_64(summary.as_bytes()))
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
fn decode_embedding_blob(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

fn open_existing_graph_db(db_path: &Path) -> Result<Connection> {
    if !db_path.exists() {
        bail!(
            "graph database not found at {}. Run `cargo run -- build <repo>` first, or set REPODNA_DB_PATH to an existing graph.db.",
            db_path.display()
        );
    }

    Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .with_context(|| format!("failed to open graph database at {}", db_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::NamedTempFile;

    fn test_db() -> Result<NamedTempFile> {
        let file = NamedTempFile::new()?;
        let conn = Connection::open(file.path())?;
        conn.execute(
            "CREATE TABLE nodes (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                metadata TEXT
            )",
            [],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'Function', 'build_graph', '', ?2)",
            params![
                "function:src/main.rs:build_graph",
                r#"{"file":"src/main.rs","is_active":true}"#
            ],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'Function', 'old_graph', '', ?2)",
            params![
                "function:src/old.rs:old_graph",
                r#"{"file":"src/old.rs","is_active":false}"#
            ],
        )?;
        Ok(file)
    }

    #[test]
    fn add_function_context_updates_active_function_summary() -> Result<()> {
        let db = test_db()?;
        let response = add_function_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        assert_eq!(response.function_id, "function:src/main.rs:build_graph");
        assert_eq!(
            response.summary,
            "Builds the durable repository graph used by local tools."
        );

        let conn = Connection::open(db.path())?;
        let stored: String = conn.query_row(
            "SELECT summary FROM nodes WHERE id = ?1",
            ["function:src/main.rs:build_graph"],
            |row| row.get(0),
        )?;
        assert_eq!(
            stored,
            "Builds the durable repository graph used by local tools."
        );

        Ok(())
    }

    #[test]
    fn add_function_context_stores_summary_embedding() -> Result<()> {
        let db = test_db()?;

        add_function_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        let conn = Connection::open(db.path())?;
        let (model, dimensions, summary_hash, embedding): (String, i64, String, Vec<u8>) = conn
            .query_row(
                "SELECT model, dimensions, summary_hash, embedding
                 FROM function_summary_embeddings
                 WHERE function_id = ?1",
                ["function:src/main.rs:build_graph"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        assert_eq!(model, "nomic-ai/nomic-embed-text-v1.5");
        assert_eq!(dimensions, 3);
        assert!(!summary_hash.is_empty());
        assert_eq!(decode_embedding_blob(&embedding), vec![0.1, 0.2, 0.3]);

        Ok(())
    }

    #[test]
    fn search_functions_does_not_match_saved_summary_context() -> Result<()> {
        let db = test_db()?;
        add_function_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        let results = search_function_nodes(db.path(), "durable repository graph", 20)?;

        assert!(results.is_empty());

        Ok(())
    }

    #[test]
    fn search_functions_rejects_context_like_query() -> Result<()> {
        let db = test_db()?;

        let error = search_function_nodes(
            db.path(),
            "run main build_graph_extracts_main CLI entrypoint",
            20,
        )
        .unwrap_err();

        assert!(error.to_string().contains("search_function_contexts"));

        Ok(())
    }

    #[test]
    fn search_function_contexts_matches_summary_terms_and_skips_empty_summaries() -> Result<()> {
        let db = test_db()?;
        add_function_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        let results = search_function_contexts(db.path(), "durable local tools", 20)?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "function:src/main.rs:build_graph");
        assert_eq!(
            results[0].summary,
            "Builds the durable repository graph used by local tools."
        );

        Ok(())
    }

    #[test]
    fn add_function_context_rejects_missing_or_inactive_function() -> Result<()> {
        let db = test_db()?;

        let missing = add_function_context_with_embedder(
            db.path(),
            "function:missing",
            "No such function",
            fake_embed,
        );
        assert!(missing.is_err());
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .contains("active function node not found")
        );

        let inactive = add_function_context_with_embedder(
            db.path(),
            "function:src/old.rs:old_graph",
            "Old graph builder.",
            fake_embed,
        );
        assert!(inactive.is_err());
        assert!(
            inactive
                .unwrap_err()
                .to_string()
                .contains("active function node not found")
        );

        Ok(())
    }

    #[test]
    fn tools_report_missing_graph_database_without_creating_it() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let missing_db = dir.path().join("missing-graph.db");

        let search_error = search_function_nodes(&missing_db, "build_graph", 20).unwrap_err();
        assert!(
            search_error
                .to_string()
                .contains("graph database not found")
        );
        assert!(!missing_db.exists());

        let update_error = add_function_context_with_embedder(
            &missing_db,
            "function:src/main.rs:build_graph",
            "Build graph.",
            fake_embed,
        )
        .unwrap_err();
        assert!(
            update_error
                .to_string()
                .contains("graph database not found")
        );
        assert!(!missing_db.exists());

        Ok(())
    }

    fn fake_embed(text: &str) -> Result<Vec<f32>> {
        assert!(!text.trim().is_empty());
        Ok(vec![0.1, 0.2, 0.3])
    }
}
