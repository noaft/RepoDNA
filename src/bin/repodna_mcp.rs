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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchNodesParams {
    /// Search hint for any RepoDNA graph node. A node can be a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity. Use a concrete name, id, node type, file path, or symbol hint such as README.md, src/ingestion/mod.rs, build_graph, File, Directory, or Function.
    query: String,
    /// Maximum number of graph nodes to return. Defaults to 20 and is clamped to 1..=100.
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct FirstLookParams {
    /// Maximum number of recommended start nodes to return. Defaults to 12 and is clamped to 1..=50.
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AddNodeContextParams {
    /// Exact graph node id returned by search_nodes. This can identify a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity.
    node_id: String,
    /// Concise high-level description of what this node is for.
    summary: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct UpdateNodeDescriptionParams {
    /// Exact graph node id returned by search_nodes. This can identify a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity.
    node_id: String,
    /// Replacement high-level description of what this node is for.
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct GraphNodeResult {
    /// Durable human/tool-written context attached to this node. It may be empty when no context has been saved yet.
    summary: String,
    /// Stable graph node id. Use this exact id for follow-up tools and graph traversal.
    id: String,
    /// Node kind, such as File, Directory, Function, Struct, Interface, or GlobalVariable.
    r#type: String,
    /// Display and search name for the node. For files and directories this is usually a path; for code symbols this is usually the symbol name.
    name: String,
    /// Optional JSON metadata string with extra facts, such as file path, line number, symbol kind, deletion state, or other ingestion details.
    metadata: Option<String>,
    /// Raw SQLite FTS BM25 score from the same search index used by the graph viewer. Lower is better.
    bm25_score: f64,
    /// Human-friendly relevance derived from bm25_score. Higher is better and closer to 1.0 means more relevant.
    relevance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SearchNodesResponse {
    results: Vec<GraphNodeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct FirstLookNode {
    /// Exact graph node id to inspect and later pass to add_node_context.
    node_id: String,
    /// Node kind, such as File, Directory, Function, Struct, Interface, or GlobalVariable.
    r#type: String,
    /// Display and search name for the node.
    name: String,
    /// Existing durable summary. It is usually empty in bootstrap mode.
    summary: String,
    /// Optional JSON metadata string with extra facts such as file path or source location.
    metadata: Option<String>,
    /// Why RepoDNA recommends reading this node during first-look/bootstrap work.
    why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct FirstLookResponse {
    /// Bootstrap state for this repository graph. "bootstrap_needed" means important context is sparse; "memory_available" means saved summaries already cover a meaningful part of the graph; "empty_graph" means the graph has no nodes.
    mode: String,
    /// Human-readable reason for the selected mode.
    reason: String,
    /// Total graph nodes in the persisted RepoDNA database.
    total_nodes: i64,
    /// Nodes whose summary is non-empty.
    summarized_nodes: i64,
    /// summarized_nodes / total_nodes as a 0.0..1.0 ratio.
    summary_coverage: f64,
    /// Nodes an agent should inspect first, then summarize back into RepoDNA with add_node_context.
    recommended_start_nodes: Vec<FirstLookNode>,
    /// Next action for the agent after reading recommended_start_nodes.
    suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddNodeContextResponse {
    /// Exact graph node id whose durable context was added or replaced.
    node_id: String,
    /// Saved durable context summary for this node.
    summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct UpdateNodeDescriptionResponse {
    /// Exact graph node id whose durable description was replaced.
    node_id: String,
    /// Replacement durable description saved for this node.
    description: String,
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
                "RepoDNA is persistent repository memory for code tools. Before reading files broadly, rebuilding repository context, running wide text search, or opening many files, ask RepoDNA first. Treat this MCP server as the default first stop for recovering what the repository already knows.\n\nMemory-first decision policy:\n1. On a new or unfamiliar repository, call first_look before broad filesystem exploration. If it returns bootstrap_needed, read the recommended_start_nodes and then call add_node_context for the nodes you understand.\n2. Call search_nodes for targeted repository discovery. It accepts concrete hints and ordinary search terms: file paths, symbol names, function names, directory names, node types, exact ids, or short natural-language terms.\n3. A node is a graph landing point in RepoDNA. A node can be a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity. Search results are not final answers; inspect each result's type, name, metadata, summary, bm25_score, and relevance to decide the next read or query action.\n4. search_nodes uses the same SQLite FTS/BM25 node index as the graph viewer search, so MCP and the viewer should be easy to compare while testing.\n5. Work from the function layer upward: locate the closest relevant node, use saved summary when present, then read source only when memory is missing, stale, or too generic.\n6. If a relevant node summary is missing after you inspect the source or docs, call add_node_context with the exact node_id so the next session does not rediscover it.\n7. If RepoDNA returns no relevant result, fallback to normal filesystem search and source reading.".to_string(),
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
        description = "Get a first-look bootstrap guide for an unfamiliar repository graph. Use this before broad filesystem exploration when entering a repo or when search_nodes has little saved summary context. It reports summary coverage and recommends important nodes to read first; after reading them, call add_node_context with each exact node_id so future sessions inherit the context."
    )]
    async fn first_look(
        &self,
        Parameters(FirstLookParams { limit }): Parameters<FirstLookParams>,
    ) -> Result<Json<FirstLookResponse>, String> {
        first_look(&self.db_path, limit.unwrap_or(12))
            .map(Json)
            .map_err(|err| err.to_string())
    }

    #[tool(
        description = "Search RepoDNA's graph for code/source nodes using the same SQLite FTS/BM25 index as the graph viewer search. A node is a graph landing point and can be a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity. Use this before reading files broadly. Query with partial names, paths, node types, symbols, exact node ids, or short natural-language terms, for example 'build_graph', 'src/api/mod.rs', 'README.md', 'File', 'Directory', 'Function', or 'graph build'. Results are not final answers: inspect each result's type, name, metadata, summary, bm25_score, and relevance to decide the next read/query action."
    )]
    async fn search_nodes(
        &self,
        Parameters(SearchNodesParams { query, limit }): Parameters<SearchNodesParams>,
    ) -> Result<Json<SearchNodesResponse>, String> {
        search_graph_nodes(&self.db_path, &query, limit.unwrap_or(20))
            .map(|results| Json(SearchNodesResponse { results }))
            .map_err(|err| err.to_string())
    }

    #[tool(
        description = "Add or replace durable context for any RepoDNA graph node. A node can be a File, Directory, Function, Struct, Interface, GlobalVariable, or future code entity. Use this after search_nodes finds a relevant node but its summary is empty, stale, or too generic, and you have inspected enough source/docs to summarize what that node is for. Requires an exact node_id from search_nodes and a concise summary."
    )]
    async fn add_node_context(
        &self,
        Parameters(AddNodeContextParams { node_id, summary }): Parameters<AddNodeContextParams>,
    ) -> Result<Json<AddNodeContextResponse>, String> {
        add_node_context(&self.db_path, &node_id, &summary)
            .map(Json)
            .map_err(|err| err.to_string())
    }

    #[tool(
        description = "Update the durable description for any existing RepoDNA graph node and regenerate its summary embedding. Use this when a saved node description is stale, incomplete, or wrong. Requires an exact node_id from search_nodes and a replacement description."
    )]
    async fn update_node_description(
        &self,
        Parameters(UpdateNodeDescriptionParams {
            node_id,
            description,
        }): Parameters<UpdateNodeDescriptionParams>,
    ) -> Result<Json<UpdateNodeDescriptionResponse>, String> {
        update_node_description(&self.db_path, &node_id, &description)
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

fn search_graph_nodes(db_path: &Path, query: &str, limit: usize) -> Result<Vec<GraphNodeResult>> {
    let conn = open_existing_graph_db(db_path)?;
    let trimmed = query.trim();
    validate_node_lookup_query(trimmed)?;
    let fts_query = format_fts_query(trimmed)?;
    ensure_nodes_fts_index(&conn)?;
    let safe_limit = limit.clamp(1, 100) as i64;

    let mut stmt = conn.prepare(
        "SELECT
            COALESCE(n.summary, ''),
            n.id,
            n.type,
            n.name,
            n.metadata,
            bm25(nodes_fts) AS score
         FROM nodes_fts
         JOIN nodes n ON n.id = nodes_fts.id
         WHERE nodes_fts MATCH ?1
         ORDER BY score ASC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![fts_query, safe_limit], |row| {
        let bm25_score: f64 = row.get(5)?;
        let relevance = 1.0 / (1.0 + bm25_score.abs());
        Ok(GraphNodeResult {
            summary: row.get(0)?,
            id: row.get(1)?,
            r#type: row.get(2)?,
            name: row.get(3)?,
            metadata: row.get(4)?,
            bm25_score,
            relevance,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }

    Ok(items)
}

fn validate_node_lookup_query(query: &str) -> Result<()> {
    if query.is_empty() {
        bail!("search_nodes query must not be empty");
    }

    Ok(())
}

fn format_fts_query(query: &str) -> Result<String> {
    let terms = query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|term| !term.trim().is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        bail!("search_nodes query must contain at least one searchable term");
    }

    Ok(terms.join(" OR "))
}

fn ensure_nodes_fts_index(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts
         USING fts5(id, name, metadata, tokenize = 'unicode61')",
        [],
    )?;
    conn.execute("DELETE FROM nodes_fts", [])?;
    conn.execute(
        "INSERT INTO nodes_fts(id, name, metadata)
         SELECT id, name, type || ' ' || COALESCE(metadata, '') FROM nodes",
        [],
    )?;
    Ok(())
}

fn first_look(db_path: &Path, limit: usize) -> Result<FirstLookResponse> {
    let conn = open_existing_graph_db(db_path)?;
    let (total_nodes, summarized_nodes): (i64, i64) = conn.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN TRIM(COALESCE(summary, '')) <> '' THEN 1 ELSE 0 END), 0)
         FROM nodes",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let summary_coverage = if total_nodes == 0 {
        0.0
    } else {
        summarized_nodes as f64 / total_nodes as f64
    };
    let mode = if total_nodes == 0 {
        "empty_graph"
    } else if summary_coverage < 0.2 {
        "bootstrap_needed"
    } else {
        "memory_available"
    };
    let reason = match mode {
        "empty_graph" => {
            "The graph database has no nodes yet. Run `cargo run -- build <repo>` first."
        }
        "bootstrap_needed" => {
            "The graph exists, but durable node summaries are sparse. The first agent should read important nodes and save context for future sessions."
        }
        _ => "RepoDNA already has a meaningful amount of saved node context. Use summaries first, then search/read as needed.",
    }
    .to_string();

    let safe_limit = limit.clamp(1, 50) as i64;
    let mut stmt = conn.prepare(
        "SELECT
            id,
            type,
            name,
            COALESCE(summary, ''),
            metadata
         FROM nodes
         WHERE TRIM(COALESCE(summary, '')) = ''
         ORDER BY
            CASE
                WHEN LOWER(name) = 'agents.md' OR LOWER(name) LIKE '%/agents.md' THEN 0
                WHEN LOWER(name) = 'readme.md' OR LOWER(name) LIKE '%/readme.md' THEN 1
                WHEN LOWER(name) = 'cargo.toml' OR LOWER(name) LIKE '%/cargo.toml' THEN 2
                WHEN LOWER(name) = 'package.json' OR LOWER(name) LIKE '%/package.json' THEN 2
                WHEN LOWER(name) = 'src/main.rs' OR LOWER(name) LIKE '%/src/main.rs' THEN 3
                WHEN LOWER(name) LIKE '%mcp%' THEN 4
                WHEN LOWER(name) LIKE '%api%' THEN 5
                WHEN LOWER(name) LIKE '%ingestion%' THEN 6
                WHEN LOWER(name) LIKE '%setting%' OR LOWER(name) LIKE '%config%' THEN 7
                WHEN type = 'File' THEN 8
                WHEN type = 'Directory' THEN 9
                WHEN type = 'Function' THEN 10
                ELSE 11
            END,
            name ASC
         LIMIT ?1",
    )?;

    let rows = stmt.query_map([safe_limit], |row| {
        let node_id: String = row.get(0)?;
        let r#type: String = row.get(1)?;
        let name: String = row.get(2)?;
        let summary: String = row.get(3)?;
        let metadata: Option<String> = row.get(4)?;
        let why = first_look_reason_for_node(&node_id, &r#type, &name);

        Ok(FirstLookNode {
            node_id,
            r#type,
            name,
            summary,
            metadata,
            why,
        })
    })?;

    let mut recommended_start_nodes = Vec::new();
    for row in rows {
        recommended_start_nodes.push(row?);
    }

    let suggested_action = if mode == "empty_graph" {
        "Build the graph first, then call first_look again.".to_string()
    } else if mode == "bootstrap_needed" {
        "Read the recommended_start_nodes in order. For each node you genuinely understand, call add_node_context with that exact node_id and a concise summary."
            .to_string()
    } else {
        "Use existing summaries first. When you inspect a stale or empty node, call add_node_context or update_node_description with the exact node_id."
            .to_string()
    };

    Ok(FirstLookResponse {
        mode: mode.to_string(),
        reason,
        total_nodes,
        summarized_nodes,
        summary_coverage,
        recommended_start_nodes,
        suggested_action,
    })
}

fn first_look_reason_for_node(node_id: &str, node_type: &str, name: &str) -> String {
    let lower_name = name.to_ascii_lowercase();
    let lower_id = node_id.to_ascii_lowercase();

    if lower_name == "agents.md" || lower_name.ends_with("/agents.md") {
        "Agent operating rules usually live here; read this before changing behavior.".to_string()
    } else if lower_name == "readme.md" || lower_name.ends_with("/readme.md") {
        "README usually carries project framing, setup, and operator-facing workflow.".to_string()
    } else if lower_name == "cargo.toml"
        || lower_name.ends_with("/cargo.toml")
        || lower_name == "package.json"
        || lower_name.ends_with("/package.json")
    {
        "Manifest files reveal the project shape, dependencies, and runnable surfaces.".to_string()
    } else if lower_name == "src/main.rs" || lower_name.ends_with("/src/main.rs") {
        "Main source entrypoint often wires the CLI or runtime workflow.".to_string()
    } else if lower_name.contains("mcp") || lower_id.contains("mcp") {
        "MCP-related nodes define how other tools recover and write repository memory.".to_string()
    } else if lower_name.contains("api") || lower_id.contains("api") {
        "API nodes usually expose graph memory to local tooling.".to_string()
    } else if lower_name.contains("ingestion") || lower_id.contains("ingestion") {
        "Ingestion nodes usually build the graph and decide what memory exists.".to_string()
    } else if lower_name.contains("setting")
        || lower_name.contains("config")
        || lower_id.contains("setting")
        || lower_id.contains("config")
    {
        "Settings/config nodes control local behavior and launch reliability.".to_string()
    } else if node_type == "File" {
        "File nodes are good bootstrap anchors because they can describe a whole source or docs surface.".to_string()
    } else if node_type == "Directory" {
        "Directory nodes help map a source area before drilling into specific symbols.".to_string()
    } else if node_type == "Function" {
        "Function nodes are useful after higher-level file context points to specific behavior."
            .to_string()
    } else {
        "Unsummarized graph node; inspect it if it looks relevant, then save durable context."
            .to_string()
    }
}

fn add_node_context(
    db_path: &Path,
    node_id: &str,
    summary: &str,
) -> Result<AddNodeContextResponse> {
    add_node_context_with_embedder(db_path, node_id, summary, embeddings::embed_text)
}

fn update_node_description(
    db_path: &Path,
    node_id: &str,
    description: &str,
) -> Result<UpdateNodeDescriptionResponse> {
    update_node_description_with_embedder(db_path, node_id, description, embeddings::embed_text)
}

fn update_node_description_with_embedder(
    db_path: &Path,
    node_id: &str,
    description: &str,
    embedder: impl Fn(&str) -> Result<embeddings::EmbeddingResult>,
) -> Result<UpdateNodeDescriptionResponse> {
    let response = add_node_context_with_embedder(db_path, node_id, description, embedder)?;

    Ok(UpdateNodeDescriptionResponse {
        node_id: response.node_id,
        description: response.summary,
    })
}

fn add_node_context_with_embedder(
    db_path: &Path,
    node_id: &str,
    summary: &str,
    embedder: impl Fn(&str) -> Result<embeddings::EmbeddingResult>,
) -> Result<AddNodeContextResponse> {
    let mut conn = open_existing_graph_db(db_path)?;
    let trimmed_id = node_id.trim();
    let trimmed_summary = summary.trim();

    if trimmed_id.is_empty() {
        bail!("node_id must not be empty");
    }
    if trimmed_summary.is_empty() {
        bail!("summary must not be empty");
    }

    ensure_node_summary_embedding_schema(&conn)?;
    let node_exists: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM nodes
            WHERE id = ?1
        )",
        [trimmed_id],
        |row| row.get(0),
    )?;

    if !node_exists {
        bail!("node not found for node_id '{}'", trimmed_id);
    }

    let embedding = embedder(trimmed_summary).context("failed to embed node summary")?;
    if embedding.vector.is_empty() {
        bail!("embedding model returned an empty vector");
    }

    let dimensions = embedding.vector.len();
    let embedding_blob = encode_embedding_blob(&embedding.vector);
    let summary_hash = stable_summary_hash(trimmed_summary);

    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE nodes
         SET summary = ?2
         WHERE id = ?1",
        params![trimmed_id, trimmed_summary],
    )?;
    tx.execute(
        "INSERT INTO node_summary_embeddings (
            node_id,
            model,
            dimensions,
            summary_hash,
            embedding,
            updated_at
         )
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(node_id) DO UPDATE SET
            model = excluded.model,
            dimensions = excluded.dimensions,
            summary_hash = excluded.summary_hash,
            embedding = excluded.embedding,
            updated_at = excluded.updated_at",
        params![
            trimmed_id,
            embedding.model,
            dimensions as i64,
            summary_hash,
            embedding_blob
        ],
    )?;
    tx.commit()?;

    Ok(AddNodeContextResponse {
        node_id: trimmed_id.to_string(),
        summary: trimmed_summary.to_string(),
    })
}

fn ensure_node_summary_embedding_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS node_summary_embeddings (
            node_id TEXT PRIMARY KEY,
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
            "graph database not found at {}. Run `cargo run -- build <repo>` first with the same repo path. Prefer REPODNA_HOME for automatic per-repository storage, or set REPODNA_DB_PATH to an existing per-repo graph.db.",
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
            "CREATE VIRTUAL TABLE nodes_fts
             USING fts5(id, name, metadata, tokenize = 'unicode61')",
            [],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'Function', 'build_graph', '', ?2)",
            params![
                "function:src/main.rs:build_graph",
                r#"{"file":"src/main.rs"}"#
            ],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'Function', 'old_graph', '', ?2)",
            params!["function:src/old.rs:old_graph", r#"{"file":"src/old.rs"}"#],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'File', 'README.md', '', NULL)",
            params!["file:README.md"],
        )?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'Directory', 'src', '', NULL)",
            params!["dir:src"],
        )?;
        refresh_test_nodes_fts(&conn)?;
        Ok(file)
    }

    fn refresh_test_nodes_fts(conn: &Connection) -> Result<()> {
        conn.execute("DELETE FROM nodes_fts", [])?;
        conn.execute(
            "INSERT INTO nodes_fts(id, name, metadata)
             SELECT id, name, type || ' ' || COALESCE(metadata, '') FROM nodes",
            [],
        )?;
        Ok(())
    }

    #[test]
    fn add_node_context_updates_function_node_summary() -> Result<()> {
        let db = test_db()?;
        let response = add_node_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        assert_eq!(response.node_id, "function:src/main.rs:build_graph");
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
    fn add_node_context_updates_file_node_summary() -> Result<()> {
        let db = test_db()?;
        let response = add_node_context_with_embedder(
            db.path(),
            "file:README.md",
            "Project overview and operator-facing setup instructions.",
            fake_embed,
        )?;

        assert_eq!(response.node_id, "file:README.md");
        assert_eq!(
            response.summary,
            "Project overview and operator-facing setup instructions."
        );

        let conn = Connection::open(db.path())?;
        let stored: String = conn.query_row(
            "SELECT summary FROM nodes WHERE id = ?1",
            ["file:README.md"],
            |row| row.get(0),
        )?;
        assert_eq!(
            stored,
            "Project overview and operator-facing setup instructions."
        );

        Ok(())
    }

    #[test]
    fn add_node_context_stores_node_summary_embedding() -> Result<()> {
        let db = test_db()?;

        add_node_context_with_embedder(
            db.path(),
            "file:README.md",
            "Project overview and operator-facing setup instructions.",
            fake_embed,
        )?;

        let conn = Connection::open(db.path())?;
        let (model, dimensions, summary_hash, embedding): (String, i64, String, Vec<u8>) = conn
            .query_row(
                "SELECT model, dimensions, summary_hash, embedding
                 FROM node_summary_embeddings
                 WHERE node_id = ?1",
                ["file:README.md"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        assert_eq!(model, "test-openai-compatible-model");
        assert_eq!(dimensions, 3);
        assert!(!summary_hash.is_empty());
        assert_eq!(decode_embedding_blob(&embedding), vec![0.1, 0.2, 0.3]);

        Ok(())
    }

    #[test]
    fn add_node_context_stores_function_summary_embedding() -> Result<()> {
        let db = test_db()?;

        add_node_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Builds the durable repository graph used by local tools.",
            fake_embed,
        )?;

        let conn = Connection::open(db.path())?;
        let (model, dimensions, summary_hash, embedding): (String, i64, String, Vec<u8>) = conn
            .query_row(
                "SELECT model, dimensions, summary_hash, embedding
                 FROM node_summary_embeddings
                 WHERE node_id = ?1",
                ["function:src/main.rs:build_graph"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        assert_eq!(model, "test-openai-compatible-model");
        assert_eq!(dimensions, 3);
        assert!(!summary_hash.is_empty());
        assert_eq!(decode_embedding_blob(&embedding), vec![0.1, 0.2, 0.3]);

        Ok(())
    }

    #[test]
    fn update_node_description_replaces_summary_and_regenerates_embedding() -> Result<()> {
        let db = test_db()?;

        add_node_context_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "Old description.",
            fake_embed_from_text,
        )?;

        let conn = Connection::open(db.path())?;
        let (old_hash, old_embedding): (String, Vec<u8>) = conn.query_row(
            "SELECT summary_hash, embedding
             FROM node_summary_embeddings
             WHERE node_id = ?1",
            ["function:src/main.rs:build_graph"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        drop(conn);

        let response = update_node_description_with_embedder(
            db.path(),
            "function:src/main.rs:build_graph",
            "New description used for retrieval.",
            fake_embed_from_text,
        )?;

        assert_eq!(response.node_id, "function:src/main.rs:build_graph");
        assert_eq!(response.description, "New description used for retrieval.");

        let conn = Connection::open(db.path())?;
        let (stored_summary, new_hash, new_embedding): (String, String, Vec<u8>) = conn
            .query_row(
                "SELECT nodes.summary, node_summary_embeddings.summary_hash, node_summary_embeddings.embedding
                 FROM nodes
                 JOIN node_summary_embeddings ON node_summary_embeddings.node_id = nodes.id
                 WHERE nodes.id = ?1",
                ["function:src/main.rs:build_graph"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;

        assert_eq!(stored_summary, "New description used for retrieval.");
        assert_ne!(new_hash, old_hash);
        assert_ne!(new_embedding, old_embedding);

        Ok(())
    }

    #[test]
    fn search_graph_nodes_accepts_multi_term_queries() -> Result<()> {
        let db = test_db()?;

        let results = search_graph_nodes(db.path(), "src main", 20)?;

        assert!(!results.is_empty());

        Ok(())
    }

    #[test]
    fn search_node_schemas_explain_parameters_and_result_fields() {
        let params_schema = serde_json::to_value(super::schemars::schema_for!(SearchNodesParams))
            .expect("params schema should serialize");
        let query_description = params_schema["properties"]["query"]["description"]
            .as_str()
            .expect("query field should have a description");
        assert!(query_description.contains("File"));
        assert!(query_description.contains("Directory"));
        assert!(query_description.contains("Function"));

        let result_schema = serde_json::to_value(super::schemars::schema_for!(GraphNodeResult))
            .expect("result schema should serialize");

        for field in [
            "summary",
            "id",
            "type",
            "name",
            "metadata",
            "bm25_score",
            "relevance",
        ] {
            let description = result_schema["properties"][field]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("{field} field should have a description"));
            assert!(
                !description.trim().is_empty(),
                "{field} field description should not be empty"
            );
        }
    }

    #[test]
    fn server_instructions_define_memory_first_decision_policy() {
        let service = RepoDnaMcp::new(PathBuf::from("graph.db"));
        let info = service.get_info();
        let instructions = info
            .instructions
            .expect("server should provide workflow instructions");

        for phrase in [
            "Before reading files broadly",
            "first_look",
            "bootstrap_needed",
            "search_nodes",
            "A node is",
            "File",
            "Directory",
            "Function",
            "BM25",
            "add_node_context",
            "fallback",
        ] {
            assert!(
                instructions.contains(phrase),
                "instructions should contain '{phrase}'"
            );
        }
    }

    #[test]
    fn search_graph_nodes_matches_files_directories_and_functions() -> Result<()> {
        let db = test_db()?;

        let file_results = search_graph_nodes(db.path(), "README.md", 20)?;
        assert_eq!(file_results.len(), 1);
        assert_eq!(file_results[0].id, "file:README.md");
        assert_eq!(file_results[0].r#type, "File");

        let directory_results = search_graph_nodes(db.path(), "Directory", 20)?;
        assert_eq!(directory_results.len(), 1);
        assert_eq!(directory_results[0].id, "dir:src");
        assert_eq!(directory_results[0].r#type, "Directory");

        let function_results = search_graph_nodes(db.path(), "build_graph", 20)?;
        assert_eq!(function_results.len(), 1);
        assert_eq!(function_results[0].id, "function:src/main.rs:build_graph");
        assert_eq!(function_results[0].r#type, "Function");

        Ok(())
    }

    #[test]
    fn search_graph_nodes_uses_graph_viewer_bm25_index() -> Result<()> {
        let db = test_db()?;
        let conn = Connection::open(db.path())?;
        conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, 'File', 'src/api/mod.rs', '', ?2)",
            params!["file:src/api/mod.rs", r#"{"file":"src/api/mod.rs"}"#],
        )?;
        refresh_test_nodes_fts(&conn)?;
        drop(conn);

        let results = search_graph_nodes(db.path(), "src api", 20)?;

        assert!(!results.is_empty());
        assert_eq!(results[0].id, "file:src/api/mod.rs");
        assert!(results[0].relevance > 0.0);
        assert!(results[0].bm25_score.is_finite());

        Ok(())
    }

    #[test]
    fn first_look_reports_bootstrap_needed_for_empty_context() -> Result<()> {
        let db = test_db()?;

        let response = first_look(db.path(), 10)?;

        assert_eq!(response.mode, "bootstrap_needed");
        assert_eq!(response.total_nodes, 4);
        assert_eq!(response.summarized_nodes, 0);
        assert_eq!(response.summary_coverage, 0.0);
        assert!(response.suggested_action.contains("add_node_context"));
        assert!(
            response.recommended_start_nodes.iter().any(
                |node| node.node_id == "file:README.md" && node.why.contains("project framing")
            )
        );

        Ok(())
    }

    #[test]
    fn add_node_context_rejects_missing_node() -> Result<()> {
        let db = test_db()?;

        let missing = add_node_context_with_embedder(
            db.path(),
            "function:missing",
            "No such node",
            fake_embed,
        );
        assert!(missing.is_err());
        assert!(missing.unwrap_err().to_string().contains("node not found"));

        Ok(())
    }

    #[test]
    fn tools_report_missing_graph_database_without_creating_it() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let missing_db = dir.path().join("missing-graph.db");

        let search_error = search_graph_nodes(&missing_db, "build_graph", 20).unwrap_err();
        assert!(
            search_error
                .to_string()
                .contains("graph database not found")
        );
        assert!(!missing_db.exists());

        let update_error = add_node_context_with_embedder(
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

    fn fake_embed(text: &str) -> Result<embeddings::EmbeddingResult> {
        assert!(!text.trim().is_empty());
        Ok(embeddings::EmbeddingResult {
            model: "test-openai-compatible-model".to_string(),
            vector: vec![0.1, 0.2, 0.3],
        })
    }

    fn fake_embed_from_text(text: &str) -> Result<embeddings::EmbeddingResult> {
        assert!(!text.trim().is_empty());
        Ok(embeddings::EmbeddingResult {
            model: "test-openai-compatible-model".to_string(),
            vector: vec![
                text.len() as f32,
                text.bytes().next().unwrap_or_default() as f32,
            ],
        })
    }
}
