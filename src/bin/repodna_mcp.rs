use anyhow::{Context, Result, bail};
use git2::Repository;
#[path = "../repodna_paths.rs"]
mod repodna_paths;
#[path = "../settings.rs"]
mod settings;
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    schemars, tool, tool_handler, tool_router, transport::stdio,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchFunctionsParams {
    query: String,
    limit: Option<usize>,
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

#[derive(Clone)]
struct RepoDnaMcp {
    db_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for RepoDnaMcp {}

#[tool_router(router = tool_router)]
impl RepoDnaMcp {
    fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Search active function nodes in project and return node-shaped JSON results")]
    async fn search_functions(
        &self,
        Parameters(SearchFunctionsParams { query, limit }): Parameters<SearchFunctionsParams>,
    ) -> Result<Json<SearchFunctionsResponse>, String> {
        search_function_nodes(&self.db_path, &query, limit.unwrap_or(20))
            .map(|results| Json(SearchFunctionsResponse { results }))
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

    if !db_path.exists() {
        bail!(
            "graph database not found at {}. Run `cargo run -- build {}` first.",
            db_path.display(),
            repo_path
        );
    }

    let service = RepoDnaMcp::new(db_path).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn search_function_nodes(
    db_path: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<FunctionNodeResult>> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open graph database at {}", db_path.display()))?;
    let trimmed = query.trim();
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
