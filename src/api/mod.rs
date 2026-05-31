use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use git2::{DiffOptions, Repository, Sort};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tower_http::cors::{Any, CorsLayer};
use serde_json::Value;

#[derive(Clone)]
pub struct GraphApiState {
    pub db_path: PathBuf,
}

#[derive(Serialize)]
pub struct NodeRecord {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub metadata: Option<String>,
}

#[derive(Serialize)]
pub struct Bm25SearchResult {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub metadata: Option<String>,
    pub bm25_score: f64,
    pub relevance: f64,
}

#[derive(Serialize)]
pub struct EdgeRecord {
    pub id: i64,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub metadata: Option<String>,
}

#[derive(Serialize)]
pub struct NeighborResponse {
    pub center: NodeRecord,
    pub neighbors: Vec<NodeRecord>,
    pub edges: Vec<EdgeRecord>,
}

#[derive(Serialize)]
pub struct AuthorOwnershipItem {
    pub file_id: String,
    pub file_name: String,
    pub commit_count: i64,
}

#[derive(Serialize)]
pub struct FileInsights {
    pub commit_count: i64,
    pub commits: Vec<NodeRecord>,
    pub top_contributors: Vec<FileOwnerScore>,
    pub top_owner: Option<FileOwnerScore>,
    pub churn_score: Option<i64>,
    pub hotspot: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct FileOwnerScore {
    pub author: String,
    pub score: f64,
    pub commit_count: i64,
}

#[derive(Serialize)]
pub struct FileHotspotResponse {
    pub file_id: String,
    pub churn_score: i64,
    pub hotspot: String,
}

#[derive(Serialize)]
pub struct FunctionHotspotResponse {
    pub function_id: String,
    pub function_name: String,
    pub file: String,
    pub file_commit_count: i64,
    pub call_degree: i64,
    pub churn_score: i64,
    pub hotspot: String,
}

#[derive(Serialize)]
pub struct CoChangeResult {
    pub file_id: String,
    pub file_name: String,
    pub co_change_count: i64,
}

#[derive(Serialize)]
pub struct AuthorInsights {
    pub commit_count: i64,
    pub modified_files_count: i64,
    pub modified_files: Vec<NodeRecord>,
    pub ownership: Vec<AuthorOwnershipItem>,
}

#[derive(Serialize)]
pub struct FunctionDiffHunk {
    pub header: String,
    pub patch: String,
}

#[derive(Serialize)]
pub struct FunctionChangeItem {
    pub commit_id: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
    pub hunks: Vec<FunctionDiffHunk>,
}

#[derive(Serialize)]
pub struct FunctionInsights {
    pub file: String,
    pub function: String,
    pub line: usize,
    pub commit_count: usize,
    pub changes: Vec<FunctionChangeItem>,
}

#[derive(Serialize)]
pub struct NodeInsightsResponse {
    pub node: NodeRecord,
    pub file_insights: Option<FileInsights>,
    pub author_insights: Option<AuthorInsights>,
    pub function_insights: Option<FunctionInsights>,
}

pub fn router(db_path: PathBuf) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/nodes", get(get_nodes))
        .route("/edges", get(get_edges))
        .route("/node/{id}", get(get_node))
        .route("/node/{id}/insights", get(get_node_insights))
        .route("/neighbors/{id}", get(get_neighbors))
        .route("/search/nodes", get(search_nodes))
        .route("/search/bm25", get(search_nodes_bm25))
        .route("/query/commits-by-file/{id}", get(get_commits_by_file))
        .route("/query/files-by-commit/{id}", get(get_files_by_commit))
        .route("/query/commits-by-author/{id}", get(get_commits_by_author))
        .route("/query/author-ownership/{id}", get(get_author_ownership))
        .route("/query/top-owner/{id}", get(get_top_owner))
        .route("/query/file-hotspots", get(get_file_hotspots))
        .route("/query/function-hotspots", get(get_function_hotspots))
        .route("/query/co-change/{file}", get(get_co_change_files))
        .route("/health", get(health))
        .layer(cors)
        .with_state(GraphApiState { db_path })
}

pub async fn serve_graph_api(
    db_path: PathBuf,
    bind_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_graph_schema(&db_path)?;

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    println!("Graph API listening at http://{}", bind_addr);
    println!("Using database: {}", db_path.display());

    axum::serve(listener, router(db_path)).await?;
    Ok(())
}

fn open_connection(db_path: &Path) -> Result<Connection, (StatusCode, String)> {
    Connection::open(db_path).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to open database: {}", err),
        )
    })
}

fn ensure_graph_schema(db_path: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY,
            type TEXT NOT NULL,
            name TEXT NOT NULL,
            metadata TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
        CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);

        CREATE TABLE IF NOT EXISTS edges (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            target TEXT NOT NULL,
            relation TEXT NOT NULL,
            metadata TEXT,
            FOREIGN KEY (source) REFERENCES nodes(id) ON DELETE CASCADE,
            FOREIGN KEY (target) REFERENCES nodes(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
        CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
        CREATE INDEX IF NOT EXISTS idx_edges_relation ON edges(relation);
        CREATE INDEX IF NOT EXISTS idx_edges_source_relation ON edges(source, relation);

        CREATE UNIQUE INDEX IF NOT EXISTS uq_edges_source_target_relation
        ON edges(source, target, relation);

        CREATE TABLE IF NOT EXISTS metadata (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS uq_metadata_entity_key
        ON metadata(entity_type, entity_id, key);

        CREATE INDEX IF NOT EXISTS idx_metadata_entity
        ON metadata(entity_type, entity_id);

        CREATE INDEX IF NOT EXISTS idx_metadata_key
        ON metadata(key);

        CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts
        USING fts5(id, name, metadata, tokenize = 'unicode61');",
    )?;

    // Keep BM25 index fresh for the current database snapshot.
    conn.execute("DELETE FROM nodes_fts", [])?;
    conn.execute(
        "INSERT INTO nodes_fts(id, name, metadata)
         SELECT id, name, COALESCE(metadata, '') FROM nodes",
        [],
    )?;

    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn get_nodes(
    State(state): State<GraphApiState>,
) -> Result<Json<Vec<NodeRecord>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare("SELECT id, type, name, metadata FROM nodes ORDER BY id")
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(Json(items))
}

async fn get_edges(
    State(state): State<GraphApiState>,
) -> Result<Json<Vec<EdgeRecord>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare("SELECT id, source, target, relation, metadata FROM edges ORDER BY id")
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(EdgeRecord {
                id: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                relation: row.get(3)?,
                metadata: row.get(4)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(Json(items))
}

async fn get_node(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<NodeRecord>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let node = conn
        .query_row(
            "SELECT id, type, name, metadata FROM nodes WHERE id = ?1",
            params![id],
            |row| {
                Ok(NodeRecord {
                    id: row.get(0)?,
                    r#type: row.get(1)?,
                    name: row.get(2)?,
                    metadata: row.get(3)?,
                })
            },
        )
        .map_err(not_found_or_internal)?;

    Ok(Json(node))
}

async fn get_neighbors(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<NeighborResponse>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;

    let center = conn
        .query_row(
            "SELECT id, type, name, metadata FROM nodes WHERE id = ?1",
            params![id.clone()],
            |row| {
                Ok(NodeRecord {
                    id: row.get(0)?,
                    r#type: row.get(1)?,
                    name: row.get(2)?,
                    metadata: row.get(3)?,
                })
            },
        )
        .map_err(not_found_or_internal)?;

    let mut edge_stmt = conn
        .prepare(
            "SELECT id, source, target, relation, metadata
             FROM edges
             WHERE source = ?1 OR target = ?1
             ORDER BY id",
        )
        .map_err(internal_db_error)?;

    let edge_rows = edge_stmt
        .query_map(params![id], |row| {
            Ok(EdgeRecord {
                id: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                relation: row.get(3)?,
                metadata: row.get(4)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut edges = Vec::new();
    let mut neighbor_ids: Vec<String> = Vec::new();

    for edge_row in edge_rows {
        let edge = edge_row.map_err(internal_db_error)?;
        if edge.source != center.id {
            neighbor_ids.push(edge.source.clone());
        }
        if edge.target != center.id {
            neighbor_ids.push(edge.target.clone());
        }
        edges.push(edge);
    }

    neighbor_ids.sort();
    neighbor_ids.dedup();

    let mut neighbors = Vec::new();
    for neighbor_id in neighbor_ids {
        let neighbor = conn
            .query_row(
                "SELECT id, type, name, metadata FROM nodes WHERE id = ?1",
                params![neighbor_id],
                |row| {
                    Ok(NodeRecord {
                        id: row.get(0)?,
                        r#type: row.get(1)?,
                        name: row.get(2)?,
                        metadata: row.get(3)?,
                    })
                },
            )
            .map_err(not_found_or_internal)?;
        neighbors.push(neighbor);
    }

    Ok(Json(NeighborResponse {
        center,
        neighbors,
        edges,
    }))
}

async fn search_nodes(
    State(state): State<GraphApiState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<NodeRecord>>, (StatusCode, String)> {
    let term = query.get("q").map(String::as_str).unwrap_or("").trim();
    if term.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let conn = open_connection(&state.db_path)?;
    let pattern = format!("%{}%", term);
    let mut stmt = conn
        .prepare(
            "SELECT id, type, name, metadata FROM nodes
             WHERE id LIKE ?1 OR name LIKE ?1
             ORDER BY name
             LIMIT 50",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![pattern], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(Json(items))
}

async fn search_nodes_bm25(
    State(state): State<GraphApiState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Bm25SearchResult>>, (StatusCode, String)> {
    let term = query.get("q").map(String::as_str).unwrap_or("").trim();
    if term.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.type, n.name, n.metadata, bm25(nodes_fts) AS score
             FROM nodes_fts
             JOIN nodes n ON n.id = nodes_fts.id
             WHERE nodes_fts MATCH ?1
             ORDER BY score ASC
             LIMIT 60",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![term], |row| {
            let bm25_score: f64 = row.get(4)?;
            let relevance = 1.0 / (1.0 + bm25_score.abs());

            Ok(Bm25SearchResult {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
                bm25_score,
                relevance,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(Json(items))
}

async fn get_commits_by_file(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<NodeRecord>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    Ok(Json(get_commits_by_file_from_conn(&conn, &id)?))
}

async fn get_files_by_commit(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<NodeRecord>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.type, n.name, n.metadata
             FROM edges e
             JOIN nodes n ON n.id = e.target
             WHERE e.relation = 'MODIFIES' AND e.source = ?1 AND n.type = 'File'
             ORDER BY n.name",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![id], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut files = Vec::new();
    for row in rows {
        files.push(row.map_err(internal_db_error)?);
    }
    Ok(Json(files))
}

async fn get_commits_by_author(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<NodeRecord>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.type, n.name, n.metadata
             FROM edges e
             JOIN nodes n ON n.id = e.source
             WHERE e.relation = 'AUTHORED_BY' AND e.target = ?1 AND n.type = 'Commit'
             ORDER BY n.name",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![id], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut commits = Vec::new();
    for row in rows {
        commits.push(row.map_err(internal_db_error)?);
    }
    Ok(Json(commits))
}

async fn get_author_ownership(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<AuthorOwnershipItem>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    Ok(Json(get_author_ownership_from_conn(&conn, &id)?))
}

async fn get_top_owner(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Option<FileOwnerScore>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    Ok(Json(get_top_owner_from_metadata(&conn, &id)?))
}

async fn get_file_hotspots(
    State(state): State<GraphApiState>,
) -> Result<Json<Vec<FileHotspotResponse>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT entity_id, value
             FROM metadata
             WHERE entity_type = 'File' AND key = 'hotspot'",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map([], |row| {
            let file_id: String = row.get(0)?;
            let raw: String = row.get(1)?;
            Ok((file_id, raw))
        })
        .map_err(internal_db_error)?;

    let mut output = Vec::new();
    for row in rows {
        let (file_id, raw) = row.map_err(internal_db_error)?;
        let parsed: Value = serde_json::from_str(&raw).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid hotspot metadata: {}", err),
            )
        })?;

        output.push(FileHotspotResponse {
            file_id,
            churn_score: parsed
                .get("churn_score")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            hotspot: parsed
                .get("hotspot")
                .and_then(Value::as_str)
                .unwrap_or("Low")
                .to_string(),
        });
    }

    Ok(Json(output))
}

async fn get_function_hotspots(
    State(state): State<GraphApiState>,
) -> Result<Json<Vec<FunctionHotspotResponse>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT entity_id, value
             FROM metadata
             WHERE entity_type = 'Function' AND key = 'hotspot'",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map([], |row| {
            let function_id: String = row.get(0)?;
            let raw: String = row.get(1)?;
            Ok((function_id, raw))
        })
        .map_err(internal_db_error)?;

    let mut output = Vec::new();
    for row in rows {
        let (function_id, raw) = row.map_err(internal_db_error)?;
        let parsed: Value = serde_json::from_str(&raw).map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Invalid function hotspot metadata: {}", err),
            )
        })?;

        output.push(FunctionHotspotResponse {
            function_id,
            function_name: parsed
                .get("function")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
                .to_string(),
            file: parsed
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            file_commit_count: parsed
                .get("file_commit_count")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            call_degree: parsed
                .get("call_degree")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            churn_score: parsed
                .get("churn_score")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            hotspot: parsed
                .get("hotspot")
                .and_then(Value::as_str)
                .unwrap_or("Low")
                .to_string(),
        });
    }

    output.sort_by(|left, right| {
        right
            .churn_score
            .cmp(&left.churn_score)
            .then_with(|| left.function_name.cmp(&right.function_name))
    });

    Ok(Json(output))
}

async fn get_co_change_files(
    State(state): State<GraphApiState>,
    AxumPath(file): AxumPath<String>,
) -> Result<Json<Vec<CoChangeResult>>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;
    let Some(file_id) = resolve_file_selector_to_id(&conn, &file)? else {
        return Ok(Json(Vec::new()));
    };

    let mut stmt = conn
        .prepare(
            "SELECT
                CASE WHEN e.source = ?1 THEN e.target ELSE e.source END AS other_file_id,
                n.name,
                CAST(COALESCE(json_extract(e.metadata, '$.count'), 0) AS INTEGER) AS co_change_count
             FROM edges e
             JOIN nodes n ON n.id = CASE WHEN e.source = ?1 THEN e.target ELSE e.source END
             WHERE e.relation = 'CO_CHANGE'
               AND (e.source = ?1 OR e.target = ?1)
               AND n.type = 'File'
             ORDER BY co_change_count DESC, n.name ASC",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![file_id], |row| {
            Ok(CoChangeResult {
                file_id: row.get(0)?,
                file_name: row.get(1)?,
                co_change_count: row.get(2)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(Json(items))
}

fn resolve_file_selector_to_id(
    conn: &Connection,
    selector: &str,
) -> Result<Option<String>, (StatusCode, String)> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let by_id: Option<String> = conn
        .query_row(
            "SELECT id FROM nodes WHERE type = 'File' AND id = ?1 LIMIT 1",
            params![trimmed],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal_db_error)?;
    if by_id.is_some() {
        return Ok(by_id);
    }

    let exact_name: Option<String> = conn
        .query_row(
            "SELECT id FROM nodes WHERE type = 'File' AND name = ?1 LIMIT 1",
            params![trimmed],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal_db_error)?;
    if exact_name.is_some() {
        return Ok(exact_name);
    }

    let suffix = format!("%/{}", trimmed);
    let by_suffix: Option<String> = conn
        .query_row(
            "SELECT id
             FROM nodes
             WHERE type = 'File' AND name LIKE ?1
             ORDER BY LENGTH(name) ASC, name ASC
             LIMIT 1",
            params![suffix],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal_db_error)?;

    Ok(by_suffix)
}

fn get_author_ownership_from_conn(
    conn: &Connection,
    author_id: &str,
) -> Result<Vec<AuthorOwnershipItem>, (StatusCode, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT f.id, f.name, COUNT(*) as commit_count
             FROM edges authored
             JOIN edges modifies ON modifies.source = authored.source
             JOIN nodes f ON f.id = modifies.target
             WHERE authored.relation = 'AUTHORED_BY'
               AND authored.target = ?1
               AND modifies.relation = 'MODIFIES'
               AND f.type = 'File'
             GROUP BY f.id, f.name
             ORDER BY commit_count DESC, f.name ASC",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![author_id], |row| {
            Ok(AuthorOwnershipItem {
                file_id: row.get(0)?,
                file_name: row.get(1)?,
                commit_count: row.get(2)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }
    Ok(items)
}

async fn get_node_insights(
    State(state): State<GraphApiState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<NodeInsightsResponse>, (StatusCode, String)> {
    let conn = open_connection(&state.db_path)?;

    let node = conn
        .query_row(
            "SELECT id, type, name, metadata FROM nodes WHERE id = ?1",
            params![id],
            |row| {
                Ok(NodeRecord {
                    id: row.get(0)?,
                    r#type: row.get(1)?,
                    name: row.get(2)?,
                    metadata: row.get(3)?,
                })
            },
        )
        .map_err(not_found_or_internal)?;

    if node.r#type == "File" {
        let commit_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIES' AND target = ?1",
                params![node.id.clone()],
                |row| row.get(0),
            )
            .map_err(internal_db_error)?;

        let commits = get_commits_by_file_from_conn(&conn, &node.id)?;

        let ownership_scores = get_ownership_scores_from_metadata(&conn, &node.id)?
            .unwrap_or_else(|| get_author_ownership_by_file(&conn, &node.id).unwrap_or_default());
        let top_owner = ownership_scores.first().cloned();
        let hotspot = get_hotspot_for_file(&conn, &node.id)?;

        return Ok(Json(NodeInsightsResponse {
            node,
            file_insights: Some(FileInsights {
                commit_count,
                commits,
                top_contributors: ownership_scores,
                top_owner,
                churn_score: hotspot.as_ref().map(|item| item.churn_score),
                hotspot: hotspot.map(|item| item.hotspot),
            }),
            author_insights: None,
            function_insights: None,
        }));
    }

    if node.r#type == "Author" {
        let commit_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'AUTHORED_BY' AND target = ?1",
                params![node.id.clone()],
                |row| row.get(0),
            )
            .map_err(internal_db_error)?;

        let ownership = get_author_ownership_from_conn(&conn, &node.id)?;

        let modified_files = get_files_by_author(&conn, &node.id)?;
        let modified_files_count = modified_files.len() as i64;

        return Ok(Json(NodeInsightsResponse {
            node,
            file_insights: None,
            author_insights: Some(AuthorInsights {
                commit_count,
                modified_files_count,
                modified_files,
                ownership,
            }),
            function_insights: None,
        }));
    }

    if node.r#type == "Function" {
        let function_insights = get_function_insights(&state.db_path, &node)?;
        return Ok(Json(NodeInsightsResponse {
            node,
            file_insights: None,
            author_insights: None,
            function_insights: Some(function_insights),
        }));
    }

    Ok(Json(NodeInsightsResponse {
        node,
        file_insights: None,
        author_insights: None,
        function_insights: None,
    }))
}

fn get_commits_by_file_from_conn(
    conn: &Connection,
    file_id: &str,
) -> Result<Vec<NodeRecord>, (StatusCode, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.type, n.name, n.metadata
             FROM edges e
             JOIN nodes n ON n.id = e.source
             WHERE e.relation = 'MODIFIES' AND e.target = ?1 AND n.type = 'Commit'
             ORDER BY n.name",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![file_id], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut commits = Vec::new();
    for row in rows {
        commits.push(row.map_err(internal_db_error)?);
    }

    Ok(commits)
}

fn get_files_by_author(conn: &Connection, author_id: &str) -> Result<Vec<NodeRecord>, (StatusCode, String)> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT f.id, f.type, f.name, f.metadata
             FROM edges authored
             JOIN edges modifies ON modifies.source = authored.source
             JOIN nodes f ON f.id = modifies.target
             WHERE authored.relation = 'AUTHORED_BY'
               AND authored.target = ?1
               AND modifies.relation = 'MODIFIES'
               AND f.type = 'File'
             ORDER BY f.name",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![author_id], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                r#type: row.get(1)?,
                name: row.get(2)?,
                metadata: row.get(3)?,
            })
        })
        .map_err(internal_db_error)?;

    let mut files = Vec::new();
    for row in rows {
        files.push(row.map_err(internal_db_error)?);
    }

    Ok(files)
}

fn get_author_ownership_by_file(
    conn: &Connection,
    file_id: &str,
) -> Result<Vec<FileOwnerScore>, (StatusCode, String)> {
    let total_commit_count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT source) FROM edges WHERE relation = 'MODIFIES' AND target = ?1",
            params![file_id],
            |row| row.get(0),
        )
        .map_err(internal_db_error)?;

    let mut stmt = conn
        .prepare(
            "SELECT a.name, COUNT(*) as commit_count
             FROM edges modifies
             JOIN edges authored ON authored.source = modifies.source
             JOIN nodes a ON a.id = authored.target
             WHERE modifies.relation = 'MODIFIES'
               AND modifies.target = ?1
               AND authored.relation = 'AUTHORED_BY'
               AND a.type = 'Author'
             GROUP BY a.id, a.name
             ORDER BY commit_count DESC, a.name ASC",
        )
        .map_err(internal_db_error)?;

    let rows = stmt
        .query_map(params![file_id], |row| {
            let author_name: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            let score = if total_commit_count > 0 {
                count as f64 / total_commit_count as f64
            } else {
                0.0
            };
            Ok(FileOwnerScore {
                author: author_name,
                score: (score * 10000.0).round() / 10000.0,
                commit_count: count,
            })
        })
        .map_err(internal_db_error)?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(internal_db_error)?);
    }

    Ok(items)
}

fn get_ownership_scores_from_metadata(
    conn: &Connection,
    file_id: &str,
) -> Result<Option<Vec<FileOwnerScore>>, (StatusCode, String)> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM metadata WHERE entity_type = 'File' AND entity_id = ?1 AND key = 'ownership'",
            params![file_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal_db_error)?;

    let Some(raw_value) = raw else {
        return Ok(None);
    };

    let parsed: Value = serde_json::from_str(&raw_value)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid ownership metadata: {}", err)))?;

    let owners = parsed
        .get("owners")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| FileOwnerScore {
                    author: entry
                        .get("author")
                        .and_then(Value::as_str)
                        .unwrap_or("<unknown>")
                        .to_string(),
                    score: entry.get("score").and_then(Value::as_f64).unwrap_or(0.0),
                    commit_count: entry
                        .get("commit_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(Some(owners))
}

fn get_top_owner_from_metadata(
    conn: &Connection,
    file_id: &str,
) -> Result<Option<FileOwnerScore>, (StatusCode, String)> {
    let ownership = get_ownership_scores_from_metadata(conn, file_id)?;
    Ok(ownership.and_then(|owners| owners.into_iter().next()))
}

fn get_hotspot_for_file(
    conn: &Connection,
    file_id: &str,
) -> Result<Option<FileHotspotResponse>, (StatusCode, String)> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM metadata WHERE entity_type = 'File' AND entity_id = ?1 AND key = 'hotspot'",
            params![file_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal_db_error)?;

    let Some(raw_value) = raw else {
        return Ok(None);
    };

    let parsed: Value = serde_json::from_str(&raw_value).map_err(|err| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Invalid hotspot metadata: {}", err),
        )
    })?;

    Ok(Some(FileHotspotResponse {
        file_id: file_id.to_string(),
        churn_score: parsed
            .get("churn_score")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        hotspot: parsed
            .get("hotspot")
            .and_then(Value::as_str)
            .unwrap_or("Low")
            .to_string(),
    }))
}

fn get_function_insights(
    db_path: &Path,
    node: &NodeRecord,
) -> Result<FunctionInsights, (StatusCode, String)> {
    let metadata_raw = node.metadata.clone().unwrap_or_default();
    let metadata: Value = serde_json::from_str(&metadata_raw).unwrap_or(Value::Null);

    let function_name = node.name.clone();
    let file_path = metadata
        .get("file")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let line = metadata
        .get("line")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;

    let mut result = FunctionInsights {
        file: file_path.clone(),
        function: function_name.clone(),
        line,
        commit_count: 0,
        changes: Vec::new(),
    };

    if file_path.is_empty() {
        return Ok(result);
    }

    let repo_hint = db_path.parent().unwrap_or_else(|| Path::new("."));
    let repo = match Repository::discover(repo_hint) {
        Ok(repo) => repo,
        Err(_) => return Ok(result),
    };

    let mut revwalk = repo.revwalk().map_err(internal_git_error)?;
    revwalk.push_head().map_err(internal_git_error)?;
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(internal_git_error)?;

    let mut commit_oids = Vec::new();
    for oid_result in revwalk {
        commit_oids.push(oid_result.map_err(internal_git_error)?);
    }
    commit_oids.reverse();

    let normalized_file_path = normalize_path(&file_path);

    for oid in commit_oids {
        let commit = repo.find_commit(oid).map_err(internal_git_error)?;
        let commit_tree = commit.tree().map_err(internal_git_error)?;
        let parent_tree = if commit.parent_count() > 0 {
            Some(commit.parent(0).map_err(internal_git_error)?.tree().map_err(internal_git_error)?)
        } else {
            None
        };

        let old_span = find_function_span_in_tree(
            &repo,
            parent_tree.as_ref(),
            &normalized_file_path,
            &function_name,
        )
        .map_err(internal_git_error)?;
        let new_span = find_function_span_in_tree(
            &repo,
            Some(&commit_tree),
            &normalized_file_path,
            &function_name,
        )
        .map_err(internal_git_error)?;

        let mut options = DiffOptions::new();
        let diff = if let Some(parent_tree_ref) = parent_tree.as_ref() {
            repo.diff_tree_to_tree(Some(parent_tree_ref), Some(&commit_tree), Some(&mut options))
                .map_err(internal_git_error)?
        } else {
            repo.diff_tree_to_tree(None, Some(&commit_tree), Some(&mut options))
                .map_err(internal_git_error)?
        };

        let mut matched_hunks = Vec::<FunctionDiffHunk>::new();
        for delta_index in 0..diff.deltas().len() {
            let Some(delta) = diff.get_delta(delta_index) else {
                continue;
            };

            if !delta_touches_path(&delta, &normalized_file_path) {
                continue;
            }

            let Some(patch) = git2::Patch::from_diff(&diff, delta_index).map_err(internal_git_error)? else {
                continue;
            };

            for hunk_index in 0..patch.num_hunks() {
                let (hunk, line_count) = patch.hunk(hunk_index).map_err(internal_git_error)?;
                let header = String::from_utf8_lossy(hunk.header()).trim().to_string();

                let mut filtered_lines = Vec::<String>::new();
                for line_index in 0..line_count {
                    let line_in_hunk = patch
                        .line_in_hunk(hunk_index, line_index)
                        .map_err(internal_git_error)?;
                    let content = String::from_utf8_lossy(line_in_hunk.content()).to_string();

                    let keep_line = if old_span.is_some() || new_span.is_some() {
                        let old_in_span = old_span
                            .map(|(start, end)| {
                                line_in_hunk
                                    .old_lineno()
                                    .map(|line_no| line_no as usize)
                                    .map(|line_no| line_no >= start && line_no <= end)
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);

                        let new_in_span = new_span
                            .map(|(start, end)| {
                                line_in_hunk
                                    .new_lineno()
                                    .map(|line_no| line_no as usize)
                                    .map(|line_no| line_no >= start && line_no <= end)
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false);

                        old_in_span || new_in_span
                    } else {
                        line_contains_function_identifier(&content, &function_name)
                            || line_contains_function_identifier(&header, &function_name)
                    };

                    if keep_line {
                        filtered_lines.push(format!("{}{}", line_in_hunk.origin(), content));
                    }
                }

                if !filtered_lines.is_empty() {
                    matched_hunks.push(FunctionDiffHunk {
                        header,
                        patch: truncate_text(filtered_lines.join("").trim(), 2200),
                    });
                }
            }
        }

        if !matched_hunks.is_empty() {
            result.changes.push(FunctionChangeItem {
                commit_id: commit.id().to_string(),
                message: commit.summary().unwrap_or("<no message>").to_string(),
                author: commit.author().name().unwrap_or("<unknown>").to_string(),
                timestamp: commit.time().seconds(),
                hunks: matched_hunks,
            });
        }

        if result.changes.len() >= 25 {
            break;
        }
    }

    result.commit_count = result.changes.len();
    Ok(result)
}

fn delta_touches_path(delta: &git2::DiffDelta<'_>, normalized_target_path: &str) -> bool {
    delta
        .new_file()
        .path()
        .map(|path| normalize_path(&path.to_string_lossy()) == normalized_target_path)
        .unwrap_or(false)
        || delta
            .old_file()
            .path()
            .map(|path| normalize_path(&path.to_string_lossy()) == normalized_target_path)
            .unwrap_or(false)
}

fn normalize_path(input: &str) -> String {
    input.replace('\\', "/")
}

fn truncate_text(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }

    let mut output = input[..max_len].to_string();
    output.push_str("\n...<truncated>");
    output
}

fn find_function_span_in_tree(
    repo: &Repository,
    tree: Option<&git2::Tree<'_>>,
    file_path: &str,
    function_name: &str,
) -> Result<Option<(usize, usize)>, git2::Error> {
    let Some(tree) = tree else {
        return Ok(None);
    };

    let content = match read_file_content_from_tree(repo, tree, file_path)? {
        Some(content) => content,
        None => return Ok(None),
    };

    Ok(find_function_span_in_content(&content, function_name))
}

fn read_file_content_from_tree(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    file_path: &str,
) -> Result<Option<String>, git2::Error> {
    let tree_path = Path::new(file_path);
    let entry = match tree.get_path(tree_path) {
        Ok(entry) => entry,
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let object = entry.to_object(repo)?;
    let Some(blob) = object.as_blob() else {
        return Ok(None);
    };

    Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
}

fn find_function_span_in_content(content: &str, function_name: &str) -> Option<(usize, usize)> {
    let fn_decl_re = regex::Regex::new(
        r"^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    )
    .ok()?;

    let mut brace_depth = 0i32;
    let mut pending_name: Option<String> = None;
    let mut active_name: Option<String> = None;
    let mut active_start_depth = 0i32;
    let mut active_start_line = 0usize;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.split("//").next().unwrap_or(raw_line);

        if let Some(caps) = fn_decl_re.captures(line) {
            if let Some(name_match) = caps.get(1) {
                pending_name = Some(name_match.as_str().to_string());
            }
        }

        let open_count = line.chars().filter(|ch| *ch == '{').count() as i32;
        let close_count = line.chars().filter(|ch| *ch == '}').count() as i32;

        if let Some(name) = pending_name.clone() {
            if open_count > 0 {
                active_name = Some(name);
                active_start_depth = brace_depth;
                active_start_line = line_number;
                pending_name = None;
            }
        }

        brace_depth += open_count - close_count;

        if let Some(name) = active_name.clone() {
            if brace_depth <= active_start_depth {
                if name == function_name {
                    return Some((active_start_line, line_number));
                }
                active_name = None;
            }
        }
    }

    None
}

fn line_contains_function_identifier(line: &str, function_name: &str) -> bool {
    if function_name.is_empty() {
        return false;
    }

    let mut start_index = 0usize;
    while let Some(found) = line[start_index..].find(function_name) {
        let absolute = start_index + found;
        let end = absolute + function_name.len();

        let left_ok = absolute == 0
            || !line[..absolute]
                .chars()
                .next_back()
                .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .unwrap_or(false);
        let right_ok = end == line.len()
            || !line[end..]
                .chars()
                .next()
                .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .unwrap_or(false);

        if left_ok && right_ok {
            return true;
        }

        start_index = end;
    }

    false
}

fn internal_git_error(err: git2::Error) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Git query failed: {}", err),
    )
}

fn internal_db_error(err: rusqlite::Error) -> (StatusCode, String) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Database query failed: {}", err),
    )
}

fn not_found_or_internal(err: rusqlite::Error) -> (StatusCode, String) {
    match err {
        rusqlite::Error::QueryReturnedNoRows => (StatusCode::NOT_FOUND, "Node not found".to_string()),
        other => internal_db_error(other),
    }
}
