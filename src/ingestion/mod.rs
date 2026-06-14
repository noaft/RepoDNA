use crate::repodna_paths;
use git2::Repository;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Node model for graph storage.
pub struct FileNode {
    pub summary: String,
    pub id: String,
    pub node_type: String,
    pub name: String,
    pub metadata: String,
}

pub struct DirectoryNode {
    pub summary: String,
    pub id: String,
    pub node_type: String,
    pub name: String,
    pub metadata: String,
}

pub struct EdgeRecord {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub metadata: Option<String>,
}

struct FileRecord {
    id: String,
    name: String,
}

struct NodeRecord {
    id: String,
    name: String,
    metadata: String,
}

struct AuthorOwnershipScore {
    author_id: String,
    author: String,
    commit_count: i64,
    score: f64,
}

struct FileHotspotMetric {
    file_id: String,
    file_name: String,
    churn_score: f64,
    level: String,
}

struct FunctionHotspotMetric {
    function_id: String,
    function_name: String,
    file_path: String,
    function_commit_score: f64,
    call_degree: i64,
    churn_score: f64,
    level: String,
}

enum RustSymbolKind {
    Function,
    Struct,
    Interface,
    Class,
    GlobalVariable,
}

struct FunctionFrame {
    function_name: String,
    start_depth: i32,
}

#[derive(Clone)]
struct FunctionSpan {
    id: String,
    name: String,
    file_path: String,
    start_line: usize,
    end_line: usize,
}

struct RustSymbolNode {
    summary: String,
    id: String,
    node_type: String,
    name: String,
    metadata: String,
}

struct RustFileSnapshot {
    file_path: String,
    content: String,
    symbols: Vec<RustSymbolNode>,
    function_spans: Vec<FunctionSpan>,
}

impl RustSymbolKind {
    fn as_node_type(&self) -> &'static str {
        match self {
            Self::Function => "Function",
            Self::Struct => "Struct",
            Self::Interface => "Interface",
            Self::Class => "Class",
            Self::GlobalVariable => "GlobalVariable",
        }
    }

    fn as_relation_hint(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Interface => "trait",
            Self::Class => "impl",
            Self::GlobalVariable => "global",
        }
    }
}

impl RustSymbolNode {
    fn new(kind: RustSymbolKind, file_path: &str, symbol_name: &str, line: usize) -> Self {
        let node_type = kind.as_node_type().to_string();
        let symbol_id = rust_symbol_id(&node_type, file_path, symbol_name);

        let metadata = json!({
            "file": file_path,
            "line": line,
            "rust_symbol_kind": kind.as_relation_hint()
        })
        .to_string();

        Self {
            summary: String::new(),
            id: symbol_id,
            node_type,
            name: symbol_name.to_string(),
            metadata,
        }
    }

    fn new_function(
        file_path: &str,
        symbol_name: &str,
        start_line: usize,
        end_line: usize,
    ) -> Self {
        let node_type = RustSymbolKind::Function.as_node_type().to_string();
        let symbol_id = rust_function_symbol_id(file_path, symbol_name, start_line);
        let symbol_key = function_symbol_key(file_path, symbol_name, start_line);

        let metadata = json!({
            "file": file_path,
            "symbol_key": symbol_key,
            "line": start_line,
            "start_line": start_line,
            "end_line": end_line,
            "rust_symbol_kind": RustSymbolKind::Function.as_relation_hint()
        })
        .to_string();

        Self {
            summary: String::new(),
            id: symbol_id,
            node_type,
            name: symbol_name.to_string(),
            metadata,
        }
    }
}

impl FileNode {
    pub fn from_path(path: &str) -> Self {
        Self {
            summary: String::new(),
            id: format!("file_{}", sanitize_id(path)),
            node_type: "File".to_string(),
            name: path.to_string(),
            metadata: json!({
                "path": path
            })
            .to_string(),
        }
    }
}

impl DirectoryNode {
    pub fn from_path(path: &str) -> Self {
        Self {
            summary: String::new(),
            id: format!("directory_{}", sanitize_id(path)),
            node_type: "Directory".to_string(),
            name: path.to_string(),
            metadata: json!({
                "path": path
            })
            .to_string(),
        }
    }
}

/// SQLite repository for graph persistence.
pub struct CommitRepository {
    conn: Connection,
}

impl CommitRepository {
    /// Open a SQLite connection and ensure required schema exists.
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        let repository = Self { conn };
        repository.ensure_schema()?;
        Ok(repository)
    }

    pub fn upsert_file_node(&self, node: &FileNode) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 type = excluded.type,
                 name = excluded.name,
                 summary = excluded.summary,
                 metadata = excluded.metadata",
            params![
                node.id,
                node.node_type,
                node.name,
                node.summary,
                node.metadata
            ],
        )?;

        Ok(rows_affected > 0)
    }

    pub fn upsert_directory_node(&self, node: &DirectoryNode) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 type = excluded.type,
                 name = excluded.name,
                 summary = excluded.summary,
                 metadata = excluded.metadata",
            params![
                node.id,
                node.node_type,
                node.name,
                node.summary,
                node.metadata
            ],
        )?;

        Ok(rows_affected > 0)
    }

    fn upsert_symbol_node(&self, node: &RustSymbolNode) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO nodes (id, type, name, summary, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                 type = excluded.type,
                 name = excluded.name,
                 summary = excluded.summary,
                 metadata = excluded.metadata",
            params![
                node.id,
                node.node_type,
                node.name,
                node.summary,
                node.metadata
            ],
        )?;

        Ok(rows_affected > 0)
    }

    pub fn upsert_edge(&self, edge: &EdgeRecord) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO edges (source, target, relation, metadata)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source, target, relation) DO NOTHING",
            params![edge.source, edge.target, edge.relation, edge.metadata],
        )?;

        Ok(rows_affected > 0)
    }

    pub fn remove_file_to_function_contains_edges(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM edges
             WHERE relation = 'CONTAINS'
               AND source IN (SELECT id FROM nodes WHERE type = 'File')
               AND target IN (SELECT id FROM nodes WHERE type = 'Function')",
            [],
        )
    }

    pub fn remove_edges_by_relation(&self, relation: &str) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM edges WHERE relation = ?1", params![relation])
    }

    pub fn remove_nodes_by_type(&self, node_type: &str) -> rusqlite::Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM nodes WHERE type = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![node_type], |row| row.get::<_, String>(0))?;

        let mut node_ids = Vec::new();
        for row in rows {
            node_ids.push(row?);
        }

        for node_id in node_ids {
            self.delete_node_and_references(&node_id, node_type)?;
        }

        Ok(())
    }

    pub fn node_count_by_type(&self, node_type: &str) -> rusqlite::Result<usize> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = ?1",
                params![node_type],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
    }

    pub fn edge_count_by_relation(&self, relation: &str) -> rusqlite::Result<usize> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = ?1",
                params![relation],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count as usize)
    }

    pub fn get_node_name(&self, node_id: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT name FROM nodes WHERE id = ?1",
                params![node_id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn upsert_metadata(
        &self,
        entity_type: &str,
        entity_id: &str,
        key: &str,
        value: &str,
    ) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO metadata (entity_type, entity_id, key, value, updated_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))
             ON CONFLICT(entity_type, entity_id, key)
             DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![entity_type, entity_id, key, value],
        )?;

        Ok(rows_affected > 0)
    }

    pub fn compute_and_store_file_ownership(&self) -> rusqlite::Result<usize> {
        let files = self.get_all_files()?;
        for file in &files {
            let total_commit_count = self.get_total_commits_for_file(&file.id)?;
            let owners = self.get_file_ownership_breakdown(&file.id, total_commit_count)?;

            let ownership_json = json!({
                "file_id": file.id,
                "file": file.name,
                "total_commit_count": total_commit_count,
                "owners": owners.iter().map(|owner| {
                    json!({
                        "author_id": owner.author_id,
                        "author": owner.author,
                        "score": owner.score,
                        "commit_count": owner.commit_count
                    })
                }).collect::<Vec<_>>(),
                "top_owner": owners.first().map(|owner| json!({
                    "author_id": owner.author_id,
                    "author": owner.author,
                    "score": owner.score,
                    "commit_count": owner.commit_count
                }))
            })
            .to_string();

            let _ = self.upsert_metadata("File", &file.id, "ownership", &ownership_json)?;
        }

        Ok(files.len())
    }

    pub fn compute_and_store_file_hotspots(&self) -> rusqlite::Result<usize> {
        let files = self.get_all_files()?;
        if files.is_empty() {
            return Ok(0);
        }

        let mut churn_scores = Vec::<f64>::new();
        let mut raw_rows = Vec::<(String, String, f64)>::new();
        for file in &files {
            let churn_score = self.get_time_decay_commit_score(&file.id, "MODIFIES")?;
            churn_scores.push(churn_score);
            raw_rows.push((file.id.clone(), file.name.clone(), churn_score));
        }

        churn_scores
            .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let len = churn_scores.len();
        let low_threshold = churn_scores[(len.saturating_sub(1) * 33) / 100];
        let high_threshold = churn_scores[(len.saturating_sub(1) * 66) / 100];

        let mut metrics = Vec::<FileHotspotMetric>::new();
        for (file_id, file_name, churn_score) in raw_rows {
            let level = if churn_score <= 0.0 {
                "Low"
            } else if churn_score >= high_threshold && high_threshold > 0.0 {
                "High"
            } else if churn_score >= low_threshold && low_threshold > 0.0 {
                "Medium"
            } else {
                "Low"
            }
            .to_string();

            metrics.push(FileHotspotMetric {
                file_id,
                file_name,
                churn_score,
                level,
            });
        }

        for metric in metrics {
            let value = json!({
                "file_id": metric.file_id,
                "file": metric.file_name,
                "churn_score": metric.churn_score,
                "hotspot_formula": "e^(-days / half_life)",
                "half_life_days": hotspot_half_life_days(),
                "hotspot": metric.level
            })
            .to_string();

            let _ = self.upsert_metadata("File", &metric.file_id, "hotspot", &value)?;
        }

        Ok(files.len())
    }

    pub fn compute_and_store_function_hotspots(&self) -> rusqlite::Result<usize> {
        let functions = self.get_all_functions()?;
        if functions.is_empty() {
            return Ok(0);
        }

        let mut churn_scores = Vec::<f64>::new();
        let mut raw_metrics = Vec::<FunctionHotspotMetric>::new();

        for function in &functions {
            let file_path = extract_file_path_from_metadata(&function.metadata);
            let function_commit_score =
                self.get_time_decay_commit_score(&function.id, "MODIFIED")?;
            let call_degree = self.get_function_call_degree(&function.id)?;
            let churn_score = function_commit_score;
            churn_scores.push(churn_score);

            raw_metrics.push(FunctionHotspotMetric {
                function_id: function.id.clone(),
                function_name: function.name.clone(),
                file_path,
                function_commit_score,
                call_degree,
                churn_score,
                level: "Low".to_string(),
            });
        }

        churn_scores
            .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let len = churn_scores.len();
        let low_threshold = churn_scores[(len.saturating_sub(1) * 33) / 100];
        let high_threshold = churn_scores[(len.saturating_sub(1) * 66) / 100];

        for metric in &mut raw_metrics {
            metric.level = if metric.churn_score <= 0.0 {
                "Low"
            } else if metric.churn_score >= high_threshold && high_threshold > 0.0 {
                "High"
            } else if metric.churn_score >= low_threshold && low_threshold > 0.0 {
                "Medium"
            } else {
                "Low"
            }
            .to_string();

            let value = json!({
                "function_id": metric.function_id,
                "function": metric.function_name,
                "file": metric.file_path,
                "function_commit_score": metric.function_commit_score,
                "call_degree": metric.call_degree,
                "churn_score": metric.churn_score,
                "hotspot_formula": "e^(-days / half_life)",
                "half_life_days": hotspot_half_life_days(),
                "hotspot": metric.level
            })
            .to_string();

            let _ = self.upsert_metadata("Function", &metric.function_id, "hotspot", &value)?;
        }

        Ok(raw_metrics.len())
    }

    fn get_all_files(&self) -> rusqlite::Result<Vec<FileRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM nodes WHERE type = 'File' ORDER BY name")?;

        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;

        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }

        Ok(files)
    }

    fn get_all_directories(&self) -> rusqlite::Result<Vec<FileRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name FROM nodes WHERE type = 'Directory' ORDER BY name")?;

        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;

        let mut directories = Vec::new();
        for row in rows {
            directories.push(row?);
        }

        Ok(directories)
    }

    fn prune_files_not_in_paths(&self, active_paths: &HashSet<String>) -> rusqlite::Result<()> {
        let files = self.get_all_files()?;
        for file in files {
            if !active_paths.contains(&file.name) {
                self.delete_node_and_references(&file.id, "File")?;
            }
        }

        Ok(())
    }

    fn prune_directories_not_in_paths(
        &self,
        active_paths: &HashSet<String>,
    ) -> rusqlite::Result<()> {
        let directories = self.get_all_directories()?;
        for directory in directories {
            if !active_paths.contains(&directory.name) {
                self.delete_node_and_references(&directory.id, "Directory")?;
            }
        }

        Ok(())
    }

    fn get_all_functions(&self) -> rusqlite::Result<Vec<NodeRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, COALESCE(metadata, '') FROM nodes WHERE type = 'Function' ORDER BY name")?;

        let rows = stmt.query_map([], |row| {
            Ok(NodeRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                metadata: row.get(2)?,
            })
        })?;

        let mut functions = Vec::new();
        for row in rows {
            functions.push(row?);
        }

        Ok(functions)
    }

    fn prune_functions_not_in_ids(
        &self,
        active_function_ids: &HashSet<String>,
    ) -> rusqlite::Result<()> {
        let functions = self.get_all_functions()?;
        for function in functions {
            if !active_function_ids.contains(&function.id) {
                self.delete_node_and_references(&function.id, "Function")?;
            }
        }

        Ok(())
    }

    fn delete_node_and_references(&self, node_id: &str, entity_type: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM edges WHERE source = ?1 OR target = ?1",
            params![node_id],
        )?;
        self.conn.execute(
            "DELETE FROM metadata WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, node_id],
        )?;
        self.conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![node_id])?;

        Ok(())
    }

    fn get_total_commits_for_file(&self, file_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(DISTINCT source)
             FROM edges
             WHERE relation = 'MODIFIES' AND target = ?1",
            params![file_id],
            |row| row.get(0),
        )
    }

    fn get_function_call_degree(&self, function_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*)
             FROM edges
             WHERE relation = 'CALLS' AND (source = ?1 OR target = ?1)",
            params![function_id],
            |row| row.get(0),
        )
    }

    fn get_total_commits_for_function(&self, function_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(DISTINCT source)
             FROM edges
             WHERE relation = 'MODIFIED' AND target = ?1",
            params![function_id],
            |row| row.get(0),
        )
    }

    fn get_time_decay_commit_score(
        &self,
        target_id: &str,
        relation: &str,
    ) -> rusqlite::Result<f64> {
        let now = current_unix_seconds();
        let half_life = hotspot_half_life_days();
        let mut stmt = self.conn.prepare(
            "SELECT CAST(COALESCE(json_extract(n.metadata, '$.timestamp'), 0) AS INTEGER)
             FROM edges e
             JOIN nodes n ON n.id = e.source
             WHERE e.relation = ?1 AND e.target = ?2 AND n.type = 'Commit'",
        )?;

        let rows = stmt.query_map(params![relation, target_id], |row| row.get::<_, i64>(0))?;

        let mut score = 0.0f64;
        for row in rows {
            let timestamp = row?;
            if timestamp <= 0 {
                continue;
            }

            let age_seconds = (now - timestamp).max(0) as f64;
            let age_days = age_seconds / 86_400.0;
            score += (-(age_days / half_life)).exp();
        }

        Ok(score)
    }

    fn get_file_ownership_breakdown(
        &self,
        file_id: &str,
        total_commit_count: i64,
    ) -> rusqlite::Result<Vec<AuthorOwnershipScore>> {
        let mut stmt = self.conn.prepare(
            "SELECT a.id, a.name, COUNT(DISTINCT m.source) as commit_count
             FROM edges m
             JOIN edges ab ON ab.source = m.source AND ab.relation = 'AUTHORED_BY'
             JOIN nodes a ON a.id = ab.target AND a.type = 'Author'
             WHERE m.relation = 'MODIFIES' AND m.target = ?1
             GROUP BY a.id, a.name
             ORDER BY commit_count DESC, a.name ASC",
        )?;

        let rows = stmt.query_map(params![file_id], |row| {
            let commit_count: i64 = row.get(2)?;
            let score = if total_commit_count > 0 {
                commit_count as f64 / total_commit_count as f64
            } else {
                0.0
            };

            Ok(AuthorOwnershipScore {
                author_id: row.get(0)?,
                author: row.get(1)?,
                commit_count,
                score: (score * 10000.0).round() / 10000.0,
            })
        })?;

        let mut owners = Vec::new();
        for row in rows {
            owners.push(row?);
        }

        Ok(owners)
    }

    fn ensure_schema(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
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
            ON metadata(key);",
        )?;
        let _ = self.conn.execute(
            "ALTER TABLE nodes ADD COLUMN summary TEXT NOT NULL DEFAULT ''",
            [],
        );

        Ok(())
    }
}

/// Ingestion summary for observability and tests.
pub struct IngestionReport {
    pub scanned: usize,
    pub commit_nodes_inserted: usize,
    pub author_nodes_inserted: usize,
    pub file_nodes_inserted: usize,
    pub directory_nodes_inserted: usize,
    pub authored_by_edges_inserted: usize,
    pub modifies_edges_inserted: usize,
    pub contains_edges_inserted: usize,
    pub call_edges_inserted: usize,
    pub main_tree_edges_inserted: usize,
    pub main_flow_edges_inserted: usize,
    pub modified_function_edges_inserted: usize,
    pub co_change_pairs_processed: usize,
    pub function_nodes_inserted: usize,
    pub class_nodes_inserted: usize,
    pub struct_nodes_inserted: usize,
    pub interface_nodes_inserted: usize,
    pub global_variable_nodes_inserted: usize,
    pub ownership_files_computed: usize,
    pub hotspot_files_computed: usize,
    pub hotspot_functions_computed: usize,
    pub duplicates_skipped: usize,
    pub db_path: PathBuf,
}

#[derive(Serialize, Deserialize, Default)]
struct RepoDnaState {
    last_built_commit: String,
    last_built_ref: Option<String>,
}

impl IngestionReport {
    fn empty(db_path: PathBuf) -> Self {
        Self {
            scanned: 0,
            commit_nodes_inserted: 0,
            author_nodes_inserted: 0,
            file_nodes_inserted: 0,
            directory_nodes_inserted: 0,
            authored_by_edges_inserted: 0,
            modifies_edges_inserted: 0,
            contains_edges_inserted: 0,
            call_edges_inserted: 0,
            main_tree_edges_inserted: 0,
            main_flow_edges_inserted: 0,
            modified_function_edges_inserted: 0,
            co_change_pairs_processed: 0,
            function_nodes_inserted: 0,
            class_nodes_inserted: 0,
            struct_nodes_inserted: 0,
            interface_nodes_inserted: 0,
            global_variable_nodes_inserted: 0,
            ownership_files_computed: 0,
            hotspot_files_computed: 0,
            hotspot_functions_computed: 0,
            duplicates_skipped: 0,
            db_path,
        }
    }
}

/// Build the graph from the current working tree snapshot.
///
/// Behavior:
/// - Scans files and Rust symbols from the current checkout.
/// - Skips git history and diff-derived edges.
/// - Skips duplicates via conflict-safe upserts.
pub fn build_graph(repo_path: &str) -> Result<IngestionReport, Box<dyn std::error::Error>> {
    let repo = Repository::discover(repo_path)?;
    repodna_paths::validate_storage_configuration(&repo)?;
    let db_path = repodna_paths::resolve_graph_db_path(&repo);
    repodna_paths::ensure_storage_dir(&repo)?;
    let repository = CommitRepository::open(&db_path)?;

    repository.remove_nodes_by_type("Commit")?;
    repository.remove_nodes_by_type("Author")?;
    let _ = repository.remove_edges_by_relation("AUTHORED_BY")?;
    let _ = repository.remove_edges_by_relation("MODIFIES")?;
    let _ = repository.remove_edges_by_relation("MODIFIED")?;
    let _ = repository.remove_edges_by_relation("CO_CHANGE")?;
    let _ = repository.remove_file_to_function_contains_edges()?;
    let _ = repository.remove_edges_by_relation("CALLS")?;
    let _ = repository.remove_edges_by_relation("MAIN_TREE")?;
    let _ = repository.remove_edges_by_relation("MAIN_FLOW")?;

    let scanned = 0usize;
    let commit_nodes_inserted = 0usize;
    let author_nodes_inserted = 0usize;
    let mut file_nodes_inserted = 0usize;
    let mut directory_nodes_inserted = 0usize;
    let authored_by_edges_inserted = 0usize;
    let modifies_edges_inserted = 0usize;
    let mut contains_edges_inserted = 0usize;
    let mut call_edges_inserted = 0usize;
    let mut main_tree_edges_inserted = 0usize;
    let mut main_flow_edges_inserted = 0usize;
    let modified_function_edges_inserted = 0usize;
    let co_change_pairs_processed = 0usize;
    let mut function_nodes_inserted = 0usize;
    let mut class_nodes_inserted = 0usize;
    let mut struct_nodes_inserted = 0usize;
    let mut interface_nodes_inserted = 0usize;
    let mut global_variable_nodes_inserted = 0usize;

    if let Some(workdir) = repo.workdir() {
        let repo_files = collect_repo_files(workdir)?;
        let current_file_paths = repo_files
            .iter()
            .map(|path| {
                path.strip_prefix(workdir)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<HashSet<_>>();
        let current_directory_paths = repo_files
            .iter()
            .flat_map(|path| {
                collect_parent_directory_paths(path.strip_prefix(workdir).unwrap_or(path))
            })
            .collect::<HashSet<_>>();
        repository.prune_files_not_in_paths(&current_file_paths)?;
        repository.prune_directories_not_in_paths(&current_directory_paths)?;

        for repo_file in &repo_files {
            let file_path = repo_file.strip_prefix(workdir).unwrap_or(repo_file);
            let file_path_str = file_path.to_string_lossy().replace('\\', "/");

            let file_node = FileNode::from_path(&file_path_str);
            if repository.upsert_file_node(&file_node)? {
                file_nodes_inserted += 1;
            }

            let (directory_nodes, contains_edges) =
                build_directory_hierarchy(&file_path_str, &file_node.id);
            for directory_node in directory_nodes {
                if repository.upsert_directory_node(&directory_node)? {
                    directory_nodes_inserted += 1;
                }
            }
            for contains_edge in contains_edges {
                if repository.upsert_edge(&contains_edge)? {
                    contains_edges_inserted += 1;
                }
            }
        }

        let rust_files = collect_rust_source_files(workdir)?;
        let mut rust_snapshots = Vec::<RustFileSnapshot>::new();

        for rust_file in rust_files {
            let file_path = rust_file.strip_prefix(workdir).unwrap_or(&rust_file);
            let file_path_str = file_path.to_string_lossy().replace('\\', "/");

            let file_node = FileNode::from_path(&file_path_str);

            let content = fs::read_to_string(&rust_file)?;
            let symbols = extract_rust_symbols(&file_path_str, &content)?;
            let function_spans = extract_rust_function_spans(&file_path_str, &content)?;
            for symbol in &symbols {
                let inserted = repository.upsert_symbol_node(symbol)?;
                if inserted {
                    match symbol.node_type.as_str() {
                        "Function" => function_nodes_inserted += 1,
                        "Class" => class_nodes_inserted += 1,
                        "Struct" => struct_nodes_inserted += 1,
                        "Interface" => interface_nodes_inserted += 1,
                        "GlobalVariable" => global_variable_nodes_inserted += 1,
                        _ => {}
                    }
                }

                let contains_symbol_edge = EdgeRecord {
                    source: file_node.id.clone(),
                    target: symbol.id.clone(),
                    relation: "CONTAINS".to_string(),
                    metadata: Some(json!({ "child_type": symbol.node_type.clone() }).to_string()),
                };

                if symbol.node_type != "Function" {
                    if repository.upsert_edge(&contains_symbol_edge)? {
                        contains_edges_inserted += 1;
                    }
                }
            }

            rust_snapshots.push(RustFileSnapshot {
                file_path: file_path_str,
                content,
                symbols,
                function_spans,
            });
        }

        let (function_ids_by_name, function_id_by_file_and_name) =
            build_function_indexes(&rust_snapshots);

        for snapshot in &rust_snapshots {
            let call_edges = extract_rust_function_calls(
                &snapshot.file_path,
                &snapshot.content,
                &function_ids_by_name,
                &function_id_by_file_and_name,
            )?;

            for call_edge in call_edges {
                if repository.upsert_edge(&call_edge)? {
                    call_edges_inserted += 1;
                }
            }
        }

        let active_function_ids = rust_snapshots
            .iter()
            .flat_map(|snapshot| snapshot.function_spans.iter().map(|span| span.id.clone()))
            .collect::<HashSet<_>>();
        repository.prune_functions_not_in_ids(&active_function_ids)?;

        main_tree_edges_inserted = compute_and_store_main_tree(&repository, &rust_snapshots)?;
        main_flow_edges_inserted = compute_and_store_main_flow_tree(&repository, &rust_snapshots)?;

        repository.prune_functions_not_in_ids(&active_function_ids)?;
    }

    let ownership_files_computed = repository.compute_and_store_file_ownership()?;
    let hotspot_files_computed = repository.compute_and_store_file_hotspots()?;
    let hotspot_functions_computed = repository.compute_and_store_function_hotspots()?;

    let total_possible = scanned
        + scanned
        + file_nodes_inserted
        + directory_nodes_inserted
        + authored_by_edges_inserted
        + modifies_edges_inserted
        + contains_edges_inserted
        + call_edges_inserted
        + main_tree_edges_inserted
        + main_flow_edges_inserted
        + modified_function_edges_inserted
        + co_change_pairs_processed
        + function_nodes_inserted
        + class_nodes_inserted
        + struct_nodes_inserted
        + interface_nodes_inserted
        + global_variable_nodes_inserted;
    let inserted_total = commit_nodes_inserted
        + author_nodes_inserted
        + file_nodes_inserted
        + directory_nodes_inserted
        + authored_by_edges_inserted
        + modifies_edges_inserted
        + contains_edges_inserted
        + call_edges_inserted
        + main_tree_edges_inserted
        + main_flow_edges_inserted
        + modified_function_edges_inserted
        + co_change_pairs_processed
        + function_nodes_inserted
        + class_nodes_inserted
        + struct_nodes_inserted
        + interface_nodes_inserted
        + global_variable_nodes_inserted;

    let duplicates_skipped = total_possible.saturating_sub(inserted_total);

    write_repodna_state(&repo)?;

    Ok(IngestionReport {
        scanned,
        commit_nodes_inserted,
        author_nodes_inserted,
        file_nodes_inserted,
        directory_nodes_inserted,
        authored_by_edges_inserted,
        modifies_edges_inserted,
        contains_edges_inserted,
        call_edges_inserted,
        main_tree_edges_inserted,
        main_flow_edges_inserted,
        modified_function_edges_inserted,
        co_change_pairs_processed,
        function_nodes_inserted,
        class_nodes_inserted,
        struct_nodes_inserted,
        interface_nodes_inserted,
        global_variable_nodes_inserted,
        ownership_files_computed,
        hotspot_files_computed,
        hotspot_functions_computed,
        duplicates_skipped,
        db_path,
    })
}

/// Backward-compatible alias for previous command naming.
pub fn ingest_commits(repo_path: &str) -> Result<IngestionReport, Box<dyn std::error::Error>> {
    build_graph(repo_path)
}

fn hotspot_half_life_days() -> f64 {
    30.0
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn rust_symbol_id(node_type: &str, file_path: &str, symbol_name: &str) -> String {
    format!(
        "{}_{}_{}",
        node_type.to_lowercase(),
        sanitize_id(file_path),
        sanitize_id(symbol_name)
    )
}

pub fn update_graph(repo_path: &str) -> Result<IngestionReport, Box<dyn std::error::Error>> {
    let repo = Repository::discover(repo_path)?;
    repodna_paths::validate_storage_configuration(&repo)?;
    let db_path = repodna_paths::resolve_graph_db_path(&repo);
    repodna_paths::ensure_storage_dir(&repo)?;

    let state = read_repodna_state(&repo)?;
    let current_head = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default();

    if current_head.is_empty() {
        return Ok(IngestionReport::empty(db_path));
    }

    if state.last_built_commit.is_empty() {
        return build_graph(repo_path);
    }

    if state.last_built_commit == current_head {
        return Ok(IngestionReport::empty(db_path));
    }

    let old_oid = match git2::Oid::from_str(&state.last_built_commit) {
        Ok(oid) => oid,
        Err(_) => return rebuild_graph(repo_path),
    };
    let new_oid = match git2::Oid::from_str(&current_head) {
        Ok(oid) => oid,
        Err(_) => return rebuild_graph(repo_path),
    };

    if repo.find_commit(old_oid).is_err() {
        return rebuild_graph(repo_path);
    }

    let is_fast_forward = repo.graph_descendant_of(new_oid, old_oid).unwrap_or(false);
    if is_fast_forward {
        return build_graph(repo_path);
    }

    let merge_base = repo.merge_base(old_oid, new_oid).ok();
    if merge_base == Some(new_oid) {
        return rebuild_graph(repo_path);
    }

    rebuild_graph(repo_path)
}

pub fn rebuild_graph(repo_path: &str) -> Result<IngestionReport, Box<dyn std::error::Error>> {
    let repo = Repository::discover(repo_path)?;
    repodna_paths::validate_storage_configuration(&repo)?;
    let db_path = repodna_paths::resolve_graph_db_path(&repo);
    repodna_paths::ensure_storage_dir(&repo)?;
    if db_path.exists() {
        fs::remove_file(&db_path)?;
    }
    build_graph(repo_path)
}

fn rust_function_symbol_id(file_path: &str, symbol_name: &str, start_line: usize) -> String {
    format!(
        "function_{}_{}_l{}",
        sanitize_id(file_path),
        sanitize_id(symbol_name),
        start_line
    )
}

fn count_braces(line: &str) -> (i32, i32) {
    let mut open_count = 0i32;
    let mut close_count = 0i32;

    for ch in line.chars() {
        if ch == '{' {
            open_count += 1;
        } else if ch == '}' {
            close_count += 1;
        }
    }

    (open_count, close_count)
}

fn strip_line_comment(line: &str) -> &str {
    if let Some(index) = line.find("//") {
        &line[..index]
    } else {
        line
    }
}

fn extract_file_path_from_metadata(metadata: &str) -> String {
    serde_json::from_str::<serde_json::Value>(metadata)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("file")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        })
        .unwrap_or_default()
}

fn function_file_key(file_path: &str, function_name: &str) -> String {
    format!("{}::{}", file_path, function_name)
}

fn function_symbol_key(file_path: &str, function_name: &str, start_line: usize) -> String {
    format!("{}::{}::L{}", file_path, function_name, start_line)
}

fn build_function_indexes(
    snapshots: &[RustFileSnapshot],
) -> (HashMap<String, Vec<String>>, HashMap<String, Vec<String>>) {
    let mut by_name = HashMap::<String, Vec<String>>::new();
    let mut by_file_and_name = HashMap::<String, Vec<String>>::new();

    for snapshot in snapshots {
        for symbol in &snapshot.symbols {
            if symbol.node_type != "Function" {
                continue;
            }

            by_name
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol.id.clone());

            by_file_and_name
                .entry(function_file_key(&snapshot.file_path, &symbol.name))
                .or_default()
                .push(symbol.id.clone());
        }
    }

    (by_name, by_file_and_name)
}

fn resolve_called_function_ids(
    raw_name: &str,
    function_ids_by_name: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let simple_name = raw_name.rsplit("::").next().unwrap_or(raw_name);
    function_ids_by_name
        .get(simple_name)
        .cloned()
        .unwrap_or_default()
}

fn extract_rust_function_calls(
    file_path: &str,
    content: &str,
    function_ids_by_name: &HashMap<String, Vec<String>>,
    function_id_by_file_and_name: &HashMap<String, Vec<String>>,
) -> Result<Vec<EdgeRecord>, regex::Error> {
    let fn_decl_re = Regex::new(
        r"^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    )?;
    let call_re = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_:]*)\s*\(")?;

    let mut edges = Vec::<EdgeRecord>::new();
    let mut seen = HashSet::<(String, String)>::new();
    let mut function_stack = Vec::<FunctionFrame>::new();
    let mut pending_function_name: Option<String> = None;
    let mut brace_depth = 0i32;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_line_comment(raw_line);
        if line.trim().is_empty() {
            continue;
        }

        if let Some(caps) = fn_decl_re.captures(line) {
            if let Some(name_match) = caps.get(1) {
                pending_function_name = Some(name_match.as_str().to_string());
            }
        }

        let (open_count, close_count) = count_braces(line);

        if let Some(function_name) = pending_function_name.clone() {
            if open_count > 0 {
                function_stack.push(FunctionFrame {
                    function_name,
                    start_depth: brace_depth,
                });
                pending_function_name = None;
            }
        }

        if let Some(current_frame) = function_stack.last() {
            let source_key = function_file_key(file_path, &current_frame.function_name);
            if let Some(source_ids) = function_id_by_file_and_name.get(&source_key) {
                if source_ids.len() != 1 {
                    continue;
                }
                let source_id = &source_ids[0];
                for caps in call_re.captures_iter(line) {
                    if let Some(name_match) = caps.get(1) {
                        let callee_name = name_match.as_str();
                        let targets =
                            resolve_called_function_ids(callee_name, function_ids_by_name);
                        if targets.len() == 1 {
                            let target_id = &targets[0];
                            if source_id == target_id {
                                continue;
                            }

                            let key = (source_id.clone(), target_id.clone());
                            if seen.insert(key.clone()) {
                                edges.push(EdgeRecord {
                                    source: key.0,
                                    target: key.1,
                                    relation: "CALLS".to_string(),
                                    metadata: Some(
                                        json!({
                                            "file": file_path,
                                            "line": line_number,
                                            "callee": callee_name
                                        })
                                        .to_string(),
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }

        brace_depth += open_count - close_count;

        while let Some(current) = function_stack.last() {
            if brace_depth <= current.start_depth {
                function_stack.pop();
            } else {
                break;
            }
        }
    }

    Ok(edges)
}

fn compute_and_store_main_tree(
    repository: &CommitRepository,
    snapshots: &[RustFileSnapshot],
) -> rusqlite::Result<usize> {
    let adjacency = get_call_adjacency(repository)?;
    let mut main_roots = Vec::<String>::new();

    for snapshot in snapshots {
        for symbol in &snapshot.symbols {
            if symbol.node_type == "Function" && symbol.name == "main" {
                main_roots.push(symbol.id.clone());
            }
        }
    }

    let mut inserted = 0usize;
    for root_id in main_roots {
        let mut visited = HashSet::<String>::new();
        let mut queue = std::collections::VecDeque::<(String, usize)>::new();
        visited.insert(root_id.clone());
        queue.push_back((root_id.clone(), 0));

        while let Some((current, depth)) = queue.pop_front() {
            let Some(children) = adjacency.get(&current) else {
                continue;
            };

            for child in children {
                if !visited.insert(child.clone()) {
                    continue;
                }

                let edge = EdgeRecord {
                    source: current.clone(),
                    target: child.clone(),
                    relation: "MAIN_TREE".to_string(),
                    metadata: Some(
                        json!({
                            "root": root_id,
                            "depth": depth + 1
                        })
                        .to_string(),
                    ),
                };
                if repository.upsert_edge(&edge)? {
                    inserted += 1;
                }

                queue.push_back((child.clone(), depth + 1));
            }
        }
    }

    Ok(inserted)
}

fn compute_and_store_main_flow_tree(
    repository: &CommitRepository,
    snapshots: &[RustFileSnapshot],
) -> rusqlite::Result<usize> {
    let mut inserted = 0usize;
    let mut functions_by_file = HashMap::<String, Vec<String>>::new();
    let mut function_file_by_id = HashMap::<String, String>::new();
    let mut entry_files = Vec::<String>::new();

    for snapshot in snapshots {
        if snapshot.file_path == "src/main.rs"
            || snapshot.file_path.ends_with("/main.rs")
            || snapshot.file_path == "main.rs"
        {
            entry_files.push(snapshot.file_path.clone());
        }

        let function_ids = snapshot
            .symbols
            .iter()
            .filter(|symbol| symbol.node_type == "Function")
            .map(|symbol| {
                function_file_by_id.insert(symbol.id.clone(), snapshot.file_path.clone());
                symbol.id.clone()
            })
            .collect::<Vec<_>>();

        if !function_ids.is_empty() {
            functions_by_file.insert(snapshot.file_path.clone(), function_ids);
        }
    }

    let adjacency = get_call_adjacency(repository)?;

    let mut discovered_files = HashSet::<String>::new();
    let mut expanded_functions = HashSet::<String>::new();
    let mut queue = std::collections::VecDeque::<(String, usize)>::new();

    for entry_file in entry_files {
        let entry_file_id = FileNode::from_path(&entry_file).id;
        discovered_files.insert(entry_file.clone());

        if let Some(functions) = functions_by_file.get(&entry_file) {
            for function_id in functions {
                let edge = EdgeRecord {
                    source: entry_file_id.clone(),
                    target: function_id.clone(),
                    relation: "MAIN_FLOW".to_string(),
                    metadata: Some(
                        json!({
                            "kind": "file_to_function",
                            "depth": 1,
                            "entry_file": entry_file
                        })
                        .to_string(),
                    ),
                };
                if repository.upsert_edge(&edge)? {
                    inserted += 1;
                }
                queue.push_back((function_id.clone(), 1));
            }
        }
    }

    while let Some((function_id, depth)) = queue.pop_front() {
        if !expanded_functions.insert(function_id.clone()) {
            continue;
        }

        let Some(targets) = adjacency.get(&function_id) else {
            continue;
        };

        for target_function_id in targets {
            let Some(target_file) = function_file_by_id.get(target_function_id).cloned() else {
                continue;
            };

            let function_to_file_edge = EdgeRecord {
                source: function_id.clone(),
                target: FileNode::from_path(&target_file).id,
                relation: "MAIN_FLOW".to_string(),
                metadata: Some(
                    json!({
                        "kind": "function_to_file",
                        "depth": depth + 1,
                        "target_file": target_file
                    })
                    .to_string(),
                ),
            };
            if repository.upsert_edge(&function_to_file_edge)? {
                inserted += 1;
            }

            if discovered_files.insert(target_file.clone()) {
                if let Some(file_functions) = functions_by_file.get(&target_file) {
                    for file_function_id in file_functions {
                        let file_to_function_edge = EdgeRecord {
                            source: FileNode::from_path(&target_file).id,
                            target: file_function_id.clone(),
                            relation: "MAIN_FLOW".to_string(),
                            metadata: Some(
                                json!({
                                    "kind": "file_to_function",
                                    "depth": depth + 2,
                                    "from_file": target_file
                                })
                                .to_string(),
                            ),
                        };
                        if repository.upsert_edge(&file_to_function_edge)? {
                            inserted += 1;
                        }
                        queue.push_back((file_function_id.clone(), depth + 2));
                    }
                }
            } else {
                queue.push_back((target_function_id.clone(), depth + 1));
            }
        }
    }

    Ok(inserted)
}

fn get_call_adjacency(
    repository: &CommitRepository,
) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let mut adjacency = HashMap::<String, Vec<String>>::new();
    let mut stmt = repository.conn.prepare(
        "SELECT e.source, e.target
         FROM edges e
         JOIN nodes source_node ON source_node.id = e.source
         JOIN nodes target_node ON target_node.id = e.target
         WHERE e.relation = 'CALLS'
           AND source_node.type = 'Function'
           AND target_node.type = 'Function'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (source, target) = row?;
        adjacency.entry(source).or_default().push(target);
    }

    Ok(adjacency)
}

fn build_directory_hierarchy(
    file_path: &str,
    file_node_id: &str,
) -> (Vec<DirectoryNode>, Vec<EdgeRecord>) {
    let normalized = file_path.replace('\\', "/");
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    if parts.len() <= 1 {
        return (Vec::new(), Vec::new());
    }

    let mut directories = Vec::<DirectoryNode>::new();
    let mut contains_edges = Vec::<EdgeRecord>::new();
    let mut parent_dir_id: Option<String> = None;
    let mut prefix = String::new();

    for part in &parts[0..parts.len() - 1] {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);

        let directory_node = DirectoryNode::from_path(&prefix);
        let current_dir_id = directory_node.id.clone();
        directories.push(directory_node);

        if let Some(parent_id) = &parent_dir_id {
            contains_edges.push(EdgeRecord {
                source: parent_id.clone(),
                target: current_dir_id.clone(),
                relation: "CONTAINS".to_string(),
                metadata: Some(json!({ "child_type": "Directory" }).to_string()),
            });
        }

        parent_dir_id = Some(current_dir_id);
    }

    if let Some(last_dir_id) = parent_dir_id {
        contains_edges.push(EdgeRecord {
            source: last_dir_id,
            target: file_node_id.to_string(),
            relation: "CONTAINS".to_string(),
            metadata: Some(json!({ "child_type": "File" }).to_string()),
        });
    }

    (directories, contains_edges)
}

fn collect_parent_directory_paths(file_path: &Path) -> Vec<String> {
    let normalized = file_path.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() <= 1 {
        return Vec::new();
    }

    let mut directories = Vec::new();
    let mut prefix = String::new();
    for part in &parts[0..parts.len() - 1] {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        directories.push(prefix.clone());
    }

    directories
}

fn collect_rust_source_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(collect_repo_files(root)?
        .into_iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .filter(|path| {
            let normalized = path.to_string_lossy().replace('\\', "/");
            should_index_rust_file(&normalized)
        })
        .collect())
}

fn should_index_rust_file(file_path: &str) -> bool {
    let normalized = file_path.replace('\\', "/");

    // Temporarily exclude git-history/diff support code from the semantic graph.
    !normalized.ends_with("src/history/mod.rs") && !normalized.contains("/src/history/")
}

fn collect_repo_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name == ".git" || dir_name == "target" || dir_name == "node_modules" {
                    continue;
                }
                walk(&path, acc)?;
            } else {
                acc.push(path);
            }
        }

        Ok(())
    }

    let mut files = Vec::<PathBuf>::new();
    walk(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn extract_rust_symbols(
    file_path: &str,
    content: &str,
) -> Result<Vec<RustSymbolNode>, regex::Error> {
    let struct_re = Regex::new(r"^\s*(pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let trait_re = Regex::new(r"^\s*(pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let impl_re = Regex::new(r"^\s*impl(?:<[^>]+>)?\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let global_re = Regex::new(r"^\s*(pub\s+)?(static|const)\s+(mut\s+)?([A-Za-z_][A-Za-z0-9_]*)")?;

    let mut symbols = Vec::<RustSymbolNode>::new();
    let mut seen = HashSet::<String>::new();

    for function in extract_rust_function_spans(file_path, content)? {
        let symbol = RustSymbolNode::new_function(
            &function.file_path,
            &function.name,
            function.start_line,
            function.end_line,
        );
        if seen.insert(symbol.id.clone()) {
            symbols.push(symbol);
        }
    }

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }

        if let Some(caps) = struct_re.captures(line) {
            if let Some(name_match) = caps.get(2) {
                let symbol = RustSymbolNode::new(
                    RustSymbolKind::Struct,
                    file_path,
                    name_match.as_str(),
                    line_number,
                );
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = trait_re.captures(line) {
            if let Some(name_match) = caps.get(2) {
                let symbol = RustSymbolNode::new(
                    RustSymbolKind::Interface,
                    file_path,
                    name_match.as_str(),
                    line_number,
                );
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = impl_re.captures(line) {
            if let Some(name_match) = caps.get(1) {
                let class_name = format!("impl {}", name_match.as_str());
                let symbol =
                    RustSymbolNode::new(RustSymbolKind::Class, file_path, &class_name, line_number);
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = global_re.captures(line) {
            if let Some(name_match) = caps.get(4) {
                let symbol = RustSymbolNode::new(
                    RustSymbolKind::GlobalVariable,
                    file_path,
                    name_match.as_str(),
                    line_number,
                );
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }
    }

    Ok(symbols)
}

fn extract_rust_function_spans(
    file_path: &str,
    content: &str,
) -> Result<Vec<FunctionSpan>, regex::Error> {
    let fn_decl_re = Regex::new(
        r"^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
    )?;

    let mut pending_function_name: Option<String> = None;
    let mut pending_function_line: Option<usize> = None;
    let mut function_stack = Vec::<FunctionFrame>::new();
    let mut function_start_lines = Vec::<usize>::new();
    let mut functions = Vec::<FunctionSpan>::new();
    let mut brace_depth = 0i32;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_line_comment(raw_line);

        if let Some(caps) = fn_decl_re.captures(line) {
            if let Some(name_match) = caps.get(1) {
                pending_function_name = Some(name_match.as_str().to_string());
                pending_function_line = Some(line_number);
            }
        }

        let (open_count, close_count) = count_braces(line);

        if let Some(function_name) = pending_function_name.clone() {
            if open_count > 0 {
                function_stack.push(FunctionFrame {
                    function_name,
                    start_depth: brace_depth,
                });
                function_start_lines.push(pending_function_line.unwrap_or(line_number));
                pending_function_name = None;
                pending_function_line = None;
            }
        }

        brace_depth += open_count - close_count;

        while let Some(current) = function_stack.last() {
            if brace_depth <= current.start_depth {
                let current = function_stack.pop().expect("frame exists");
                let start_line = function_start_lines.pop().unwrap_or(line_number);
                functions.push(FunctionSpan {
                    id: rust_function_symbol_id(file_path, &current.function_name, start_line),
                    name: current.function_name,
                    file_path: file_path.to_string(),
                    start_line,
                    end_line: line_number,
                });
            } else {
                break;
            }
        }
    }

    Ok(functions)
}

fn write_repodna_state(repo: &Repository) -> io::Result<()> {
    repodna_paths::ensure_storage_dir(repo)?;
    let head = repo.head().ok();
    let head_sha = head
        .as_ref()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string())
        .unwrap_or_default();
    let head_ref = head
        .as_ref()
        .and_then(|head| head.name())
        .map(ToString::to_string);

    let state = RepoDnaState {
        last_built_commit: head_sha,
        last_built_ref: head_ref,
    };

    let raw = serde_json::to_string(&state)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;

    fs::write(repodna_paths::resolve_state_path(repo), raw)
}

fn read_repodna_state(repo: &Repository) -> io::Result<RepoDnaState> {
    let path = repodna_paths::resolve_state_path(repo);
    if !path.exists() {
        return Ok(RepoDnaState::default());
    }

    let raw = fs::read_to_string(path)?;
    serde_json::from_str::<RepoDnaState>(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use rusqlite::Connection;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn build_graph_uses_current_worktree_without_git_history_nodes_or_edges() {
        let (temp_dir, _repo) = init_repo_with_commits(&["first", "second", "third"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");
        std::fs::write(src_dir.join("lib.rs"), "fn run() {}\n")
            .expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        let db = Connection::open(report.db_path).expect("db should open");
        let commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit'",
                [],
                |row| row.get(0),
            )
            .expect("count query should succeed");
        let author_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Author'",
                [],
                |row| row.get(0),
            )
            .expect("author count should succeed");
        let file_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'File'",
                [],
                |row| row.get(0),
            )
            .expect("file count should succeed");
        let _directory_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Directory'",
                [],
                |row| row.get(0),
            )
            .expect("directory count should succeed");
        let authored_by_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'AUTHORED_BY'",
                [],
                |row| row.get(0),
            )
            .expect("authored_by count should succeed");
        let modifies_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIES'",
                [],
                |row| row.get(0),
            )
            .expect("modifies count should succeed");
        let _contains_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'CONTAINS'",
                [],
                |row| row.get(0),
            )
            .expect("contains count should succeed");
        let ownership_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE entity_type = 'File' AND key = 'ownership'",
                [],
                |row| row.get(0),
            )
            .expect("ownership count should succeed");
        let hotspot_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE entity_type = 'File' AND key = 'hotspot'",
                [],
                |row| row.get(0),
            )
            .expect("hotspot count should succeed");

        assert_eq!(report.scanned, 0);
        assert_eq!(commit_count, 0);
        assert_eq!(author_count, 0);
        assert!(file_count >= 1);
        assert_eq!(authored_by_count, 0);
        assert_eq!(modifies_count, 0);
        assert_eq!(ownership_count, file_count);
        assert_eq!(hotspot_count, file_count);
        assert_eq!(report.ownership_files_computed as i64, file_count);
        assert_eq!(report.hotspot_files_computed as i64, file_count);
    }

    #[test]
    fn build_graph_adds_non_rust_files_as_file_nodes_without_symbols() {
        let (temp_dir, _repo) = init_repo_with_commits(&["first"]);
        let src_dir = temp_dir.path().join("src");
        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");
        std::fs::create_dir_all(&config_dir).expect("config dir should be created");
        std::fs::write(src_dir.join("lib.rs"), "fn run() {}\n")
            .expect("rust file should be written");
        std::fs::write(config_dir.join("app.json"), "{\"enabled\":true}\n")
            .expect("json file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        let db = Connection::open(report.db_path).expect("db should open");
        let json_file_id = FileNode::from_path("config/app.json").id;
        let config_dir_id = DirectoryNode::from_path("config").id;
        let json_file_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1 AND type = 'File'",
                [&json_file_id],
                |row| row.get(0),
            )
            .expect("json file count should succeed");
        let contains_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE source = ?1 AND target = ?2 AND relation = 'CONTAINS'",
                [&config_dir_id, &json_file_id],
                |row| row.get(0),
            )
            .expect("contains count should succeed");
        let json_function_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Function' AND metadata LIKE '%config/app.json%'",
                [],
                |row| row.get(0),
            )
            .expect("json function count should succeed");

        assert_eq!(json_file_count, 1);
        assert_eq!(contains_count, 1);
        assert_eq!(json_function_count, 0);
    }

    #[test]
    fn build_graph_is_idempotent_and_skips_duplicates() {
        let (temp_dir, _repo) = init_repo_with_commits(&["first", "second"]);

        let first = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("first build should succeed");
        let second = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("second build should succeed");

        let db = Connection::open(second.db_path).expect("db should open");
        let commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit'",
                [],
                |row| row.get(0),
            )
            .expect("commit count query should succeed");
        let authored_by_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'AUTHORED_BY'",
                [],
                |row| row.get(0),
            )
            .expect("edge count query should succeed");

        assert_eq!(first.commit_nodes_inserted, 0);
        assert_eq!(second.commit_nodes_inserted, 0);
        assert_eq!(commit_count, 0);
        assert_eq!(authored_by_count, 0);
    }

    #[test]
    fn build_graph_extracts_rust_symbol_nodes() {
        let (temp_dir, _repo) = init_repo_with_commits(&["initial"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");

        let rust_source = r#"
pub static GLOBAL_COUNT: usize = 1;

pub struct Cache {}

pub trait Repository {
    fn get(&self);
}

impl Cache {
    pub fn allocate(&self) {}
}

fn helper() {}
"#;

        std::fs::write(src_dir.join("lib.rs"), rust_source).expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        assert!(report.function_nodes_inserted >= 2);
        assert!(report.struct_nodes_inserted >= 1);
        assert!(report.interface_nodes_inserted >= 1);
        assert!(report.class_nodes_inserted >= 1);
        assert!(report.global_variable_nodes_inserted >= 1);
        assert!(report.hotspot_functions_computed >= 2);
    }

    #[test]
    fn build_graph_marks_zero_churn_function_hotspot_as_low() {
        let (temp_dir, _repo) = init_repo_with_commits(&["initial"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");

        let rust_source = r#"
fn resolve_graph_db_path() {}
"#;

        std::fs::write(src_dir.join("single.rs"), rust_source)
            .expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        let db = Connection::open(report.db_path).expect("db should open");
        let function_id = rust_function_symbol_id("src/single.rs", "resolve_graph_db_path", 2);
        let raw: String = db
            .query_row(
                "SELECT value FROM metadata WHERE entity_type = 'Function' AND entity_id = ?1 AND key = 'hotspot'",
                params![function_id],
                |row| row.get(0),
            )
            .expect("hotspot metadata should exist");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("metadata should parse");

        assert_eq!(
            parsed
                .get("churn_score")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(-1.0),
            0.0
        );
        assert_eq!(
            parsed.get("hotspot").and_then(serde_json::Value::as_str),
            Some("Low")
        );
    }

    #[test]
    fn build_graph_extracts_function_call_edges() {
        let (temp_dir, _repo) = init_repo_with_commits(&["initial"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");

        let rust_source = r#"
fn b() {}

fn a() {
    b();
}
"#;

        std::fs::write(src_dir.join("call_graph.rs"), rust_source)
            .expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        assert!(report.call_edges_inserted >= 1);

        let db = Connection::open(report.db_path).expect("db should open");
        let a_id = rust_function_symbol_id("src/call_graph.rs", "a", 4);
        let b_id = rust_function_symbol_id("src/call_graph.rs", "b", 2);

        let call_edge_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'CALLS' AND source = ?1 AND target = ?2",
                params![a_id, b_id],
                |row| row.get(0),
            )
            .expect("calls edge count should succeed");

        let file_to_function_contains_count: i64 = db
            .query_row(
                "SELECT COUNT(*)
                 FROM edges
                 WHERE relation = 'CONTAINS'
                   AND source IN (SELECT id FROM nodes WHERE type = 'File')
                   AND target IN (?1, ?2)",
                params![a_id, b_id],
                |row| row.get(0),
            )
            .expect("file-to-function contains count should succeed");

        assert_eq!(call_edge_count, 1);
        assert_eq!(file_to_function_contains_count, 0);
    }

    #[test]
    fn build_graph_extracts_cross_file_function_call_edges() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-main-and-helper",
            &[
                (
                    "src/main.rs",
                    "mod helper;\n\nfn main() {\n    helper::run();\n}\n",
                ),
                ("src/helper.rs", "pub fn run() {}\n"),
            ],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        assert!(report.call_edges_inserted >= 1);

        let db = Connection::open(report.db_path).expect("db should open");
        let main_id = rust_function_symbol_id("src/main.rs", "main", 3);
        let run_id = rust_function_symbol_id("src/helper.rs", "run", 1);

        let cross_file_call_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'CALLS' AND source = ?1 AND target = ?2",
                params![main_id, run_id],
                |row| row.get(0),
            )
            .expect("cross-file calls edge count should succeed");

        assert_eq!(cross_file_call_count, 1);
    }

    #[test]
    fn build_graph_extracts_main_tree_edges_from_same_file_calls() {
        let (temp_dir, _repo) = init_repo_with_commits(&["initial"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");

        let rust_source = r#"
fn child() {}

fn main() {
    child();
}
"#;

        std::fs::write(src_dir.join("tree.rs"), rust_source).expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let main_id = rust_function_symbol_id("src/tree.rs", "main", 4);
        let child_id = rust_function_symbol_id("src/tree.rs", "child", 2);

        let main_tree_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MAIN_TREE' AND source = ?1 AND target = ?2",
                params![main_id, child_id],
                |row| row.get(0),
            )
            .expect("main tree edge count should succeed");

        assert_eq!(main_tree_count, 1);
    }

    #[test]
    fn build_graph_extracts_main_tree_edges_across_files() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-main-tree",
            &[
                (
                    "src/main.rs",
                    "mod helper;\n\nfn main() {\n    helper::run();\n}\n",
                ),
                (
                    "src/helper.rs",
                    "pub fn run() {\n    leaf();\n}\n\nfn leaf() {}\n",
                ),
            ],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let main_id = rust_function_symbol_id("src/main.rs", "main", 3);
        let run_id = rust_function_symbol_id("src/helper.rs", "run", 1);
        let leaf_id = rust_function_symbol_id("src/helper.rs", "leaf", 5);

        let main_to_run_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MAIN_TREE' AND source = ?1 AND target = ?2",
                params![main_id, run_id],
                |row| row.get(0),
            )
            .expect("main to run edge count should succeed");
        let run_to_leaf_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MAIN_TREE' AND source = ?1 AND target = ?2",
                params![run_id, leaf_id],
                |row| row.get(0),
            )
            .expect("run to leaf edge count should succeed");

        assert_eq!(main_to_run_count, 1);
        assert_eq!(run_to_leaf_count, 1);
    }

    #[test]
    fn build_graph_extracts_main_flow_file_function_file_structure() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-main-flow",
            &[
                (
                    "src/main.rs",
                    "mod helper;\n\nfn aux() {}\n\nfn main() {\n    helper::run();\n}\n",
                ),
                (
                    "src/helper.rs",
                    "pub fn run() {\n    leaf();\n}\n\nfn leaf() {}\nfn other() {}\n",
                ),
            ],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let main_file_id = FileNode::from_path("src/main.rs").id;
        let helper_file_id = FileNode::from_path("src/helper.rs").id;
        let aux_id = rust_function_symbol_id("src/main.rs", "aux", 3);
        let main_id = rust_function_symbol_id("src/main.rs", "main", 5);
        let run_id = rust_function_symbol_id("src/helper.rs", "run", 1);
        let leaf_id = rust_function_symbol_id("src/helper.rs", "leaf", 5);
        let other_id = rust_function_symbol_id("src/helper.rs", "other", 6);

        let main_file_to_main_functions: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges
                 WHERE relation = 'MAIN_FLOW'
                   AND source = ?1
                   AND target IN (?2, ?3)",
                params![main_file_id, aux_id, main_id],
                |row| row.get(0),
            )
            .expect("main file to functions count should succeed");
        let main_to_helper_file: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges
                 WHERE relation = 'MAIN_FLOW'
                   AND source = ?1
                   AND target = ?2",
                params![main_id, helper_file_id.clone()],
                |row| row.get(0),
            )
            .expect("main to helper file count should succeed");
        let helper_file_to_functions: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges
                 WHERE relation = 'MAIN_FLOW'
                   AND source = ?1
                   AND target IN (?2, ?3, ?4)",
                params![helper_file_id, run_id, leaf_id, other_id],
                |row| row.get(0),
            )
            .expect("helper file to functions count should succeed");

        assert_eq!(main_file_to_main_functions, 2);
        assert_eq!(main_to_helper_file, 1);
        assert_eq!(helper_file_to_functions, 3);
    }

    #[test]
    fn build_graph_keeps_same_named_functions_distinct_in_db() {
        let (temp_dir, _repo) = init_repo_with_commits(&["initial"]);
        let src_dir = temp_dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should be created");

        let rust_source = r#"
struct A;
struct B;

impl A {
    fn run(&self) {}
}

impl B {
    fn run(&self) {}
}
"#;

        std::fs::write(src_dir.join("dup.rs"), rust_source).expect("rust file should be written");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let run_a_id = rust_function_symbol_id("src/dup.rs", "run", 6);
        let run_b_id = rust_function_symbol_id("src/dup.rs", "run", 10);

        let run_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id IN (?1, ?2) AND type = 'Function'",
                params![run_a_id, run_b_id],
                |row| row.get(0),
            )
            .expect("distinct same-name function count should succeed");

        assert_eq!(run_count, 2);
    }

    #[test]
    fn build_graph_skips_file_co_change_counts_from_git_history() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "cochange-1",
            &[
                ("src/a.rs", "fn a() {}"),
                ("src/b.rs", "fn b() {}"),
                ("src/c.rs", "fn c() {}"),
            ],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "cochange-2",
            &[
                ("src/a.rs", "fn a() { let _x = 1; }"),
                ("src/b.rs", "fn b() { let _y = 2; }"),
            ],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        assert_eq!(report.co_change_pairs_processed, 0);

        let db = Connection::open(report.db_path).expect("db should open");
        let co_change_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'CO_CHANGE'",
                [],
                |row| row.get(0),
            )
            .expect("co-change count should succeed");

        assert_eq!(co_change_count, 0);
    }

    #[test]
    fn build_graph_skips_single_function_modification_from_git_diff() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-functions",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 1;\n}\n\nfn evict() {\n    let _y = 2;\n}\n",
            )],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "touch-allocate",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 3;\n}\n\nfn evict() {\n    let _y = 2;\n}\n",
            )],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let allocate_id = rust_function_symbol_id("src/lib.rs", "allocate", 1);
        let evict_id = rust_function_symbol_id("src/lib.rs", "evict", 5);
        let modified_edge_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIED' AND target IN (?1, ?2)",
                params![allocate_id, evict_id],
                |row| row.get(0),
            )
            .expect("modified edge count should succeed");

        assert_eq!(modified_edge_count, 0);
    }

    #[test]
    fn build_graph_skips_multiple_function_modifications_from_git_diff() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-functions",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 1;\n}\n\nfn evict() {\n    let _y = 2;\n}\n",
            )],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "touch-both",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 4;\n}\n\nfn evict() {\n    let _y = 5;\n}\n",
            )],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let modified_functions: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIED'",
                [],
                |row| row.get(0),
            )
            .expect("modified function count should succeed");

        assert_eq!(modified_functions, 0);
    }

    #[test]
    fn build_graph_skips_whole_file_rewrite_function_edges_from_git_diff() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-functions",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 1;\n}\n\nfn evict() {\n    let _y = 2;\n}\n",
            )],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "rewrite-file",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 10;\n    let _z = 20;\n}\n\nfn evict() {\n    let _y = 30;\n    let _w = 40;\n}\n",
            )],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let modified_functions: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIED'",
                [],
                |row| row.get(0),
            )
            .expect("modified function count should succeed");

        assert_eq!(modified_functions, 0);
    }

    #[test]
    fn build_graph_skips_function_rename_commit_edges_from_git_diff() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-old-name",
            &[("src/lib.rs", "fn allocate() {\n    let _x = 1;\n}\n")],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "rename-function",
            &[("src/lib.rs", "fn reserve() {\n    let _x = 2;\n}\n")],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let reserve_id = rust_function_symbol_id("src/lib.rs", "reserve", 1);

        let reserve_edge_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIED' AND target = ?1",
                params![reserve_id],
                |row| row.get(0),
            )
            .expect("renamed function edge count should succeed");

        assert_eq!(reserve_edge_count, 0);
    }

    #[test]
    fn build_graph_skips_noop_commits_for_function_edges() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-function",
            &[("src/lib.rs", "fn allocate() {\n    let _x = 1;\n}\n")],
        );
        commit_empty(&repo, "noop");

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let modified_functions: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'MODIFIED'",
                [],
                |row| row.get(0),
            )
            .expect("modified function count should succeed");

        assert_eq!(modified_functions, 0);
    }

    #[test]
    fn build_graph_removes_deleted_function_nodes() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-functions",
            &[(
                "src/lib.rs",
                "fn allocate() {\n    let _x = 1;\n}\n\nfn evict() {\n    let _y = 2;\n}\n",
            )],
        );
        commit_files(
            &repo,
            temp_dir.path(),
            "delete-allocate",
            &[("src/lib.rs", "fn evict() {\n    let _y = 2;\n}\n")],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let allocate_id = rust_function_symbol_id("src/lib.rs", "allocate", 1);

        let deleted_function_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1",
                params![allocate_id],
                |row| row.get(0),
            )
            .expect("deleted function count should succeed");

        assert_eq!(deleted_function_count, 0);
    }

    #[test]
    fn build_graph_removes_deleted_file_nodes() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-temp-file",
            &[("src/temp.rs", "fn temp() {\n    let _x = 1;\n}\n")],
        );
        delete_and_commit_files(&repo, temp_dir.path(), "delete-temp-file", &["src/temp.rs"]);

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let temp_file_id = FileNode::from_path("src/temp.rs").id;

        let deleted_file_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1",
                params![temp_file_id],
                |row| row.get(0),
            )
            .expect("deleted file count should succeed");

        assert_eq!(deleted_file_count, 0);
    }

    #[test]
    fn build_graph_removes_deleted_directory_nodes() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        commit_files(
            &repo,
            temp_dir.path(),
            "add-nested-file",
            &[(
                "test/function_diff_overlap/case.rs",
                "fn temp() {\n    let _x = 1;\n}\n",
            )],
        );
        delete_and_commit_files(
            &repo,
            temp_dir.path(),
            "delete-nested-file",
            &["test/function_diff_overlap/case.rs"],
        );

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let directory_id = DirectoryNode::from_path("test/function_diff_overlap").id;

        let deleted_directory_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE id = ?1",
                params![directory_id],
                |row| row.get(0),
            )
            .expect("deleted directory count should succeed");

        assert_eq!(deleted_directory_count, 0);
    }

    #[test]
    fn build_graph_writes_repodna_state_file() {
        let (temp_dir, repo) = init_repo_with_commits(&["seed"]);

        let report = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("build should succeed");

        let state_path = crate::repodna_paths::resolve_state_path(&repo);
        assert!(state_path.exists());
        assert_eq!(
            report.db_path,
            crate::repodna_paths::resolve_graph_db_path(&repo)
        );

        let raw = std::fs::read_to_string(state_path).expect("state file should be readable");
        let parsed: serde_json::Value =
            serde_json::from_str(&raw).expect("state json should parse");
        let expected_head = repo
            .head()
            .expect("head should exist")
            .target()
            .expect("head target should exist")
            .to_string();

        assert_eq!(
            parsed
                .get("last_built_commit")
                .and_then(serde_json::Value::as_str),
            Some(expected_head.as_str())
        );
    }

    #[test]
    fn update_graph_fast_forward_adds_new_commits() {
        let (temp_dir, repo) = init_repo_with_commits(&["base"]);

        let first = build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("initial build should succeed");
        let first_db = Connection::open(first.db_path).expect("db should open");
        let first_commit_count: i64 = first_db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit'",
                [],
                |row| row.get(0),
            )
            .expect("commit count should succeed");
        assert_eq!(first_commit_count, 0);

        commit_files(
            &repo,
            temp_dir.path(),
            "after-base",
            &[("src/lib.rs", "fn run() {}\n")],
        );

        let report = update_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("update should succeed");
        let db = Connection::open(report.db_path).expect("db should open");
        let commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit'",
                [],
                |row| row.get(0),
            )
            .expect("commit count should succeed");

        assert_eq!(commit_count, 0);
    }

    #[test]
    fn update_graph_rebuilds_cleanly_after_diverged_history() {
        let (temp_dir, repo) = init_repo_with_commits(&["base"]);

        let base_oid = repo
            .head()
            .expect("head should exist")
            .target()
            .expect("head target should exist");
        repo.branch(
            "feature",
            &repo
                .find_commit(base_oid)
                .expect("base commit should exist"),
            false,
        )
        .expect("branch should be created");

        checkout_branch(&repo, "feature");
        commit_files(
            &repo,
            temp_dir.path(),
            "feature-only",
            &[("src/feature.rs", "fn feature_only() {}\n")],
        );

        build_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("feature build should succeed");

        checkout_branch(&repo, "master");
        commit_files(
            &repo,
            temp_dir.path(),
            "main-only",
            &[("src/main_only.rs", "fn main_only() {}\n")],
        );

        let report = update_graph(temp_dir.path().to_str().expect("valid path"))
            .expect("update should succeed");
        let db = Connection::open(report.db_path).expect("db should open");

        let commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit'",
                [],
                |row| row.get(0),
            )
            .expect("commit count should succeed");
        let feature_commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit' AND name = 'feature-only'",
                [],
                |row| row.get(0),
            )
            .expect("feature-only commit count should succeed");
        let main_commit_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE type = 'Commit' AND name = 'main-only'",
                [],
                |row| row.get(0),
            )
            .expect("main-only commit count should succeed");

        assert_eq!(commit_count, 0);
        assert_eq!(feature_commit_count, 0);
        assert_eq!(main_commit_count, 0);
    }

    fn init_repo_with_commits(messages: &[&str]) -> (TempDir, Repository) {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let repo = Repository::init(temp_dir.path()).expect("repo should be initialized");

        for (index, message) in messages.iter().enumerate() {
            let file_content = format!("content-{}-{}", index, message);
            std::fs::write(temp_dir.path().join("history.txt"), file_content)
                .expect("file write should succeed");

            let mut git_index = repo.index().expect("index should be available");
            git_index
                .add_path(Path::new("history.txt"))
                .expect("path should be added to index");
            git_index.write().expect("index write should succeed");

            let tree_id = git_index.write_tree().expect("tree id should be created");
            let tree = repo.find_tree(tree_id).expect("tree should be found");
            let signature =
                Signature::now("Test User", "test@example.com").expect("signature should exist");

            if let Ok(head) = repo.head() {
                let parent = repo
                    .find_commit(head.target().expect("head oid should exist"))
                    .expect("parent commit should be found");

                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[&parent],
                )
                .expect("commit should succeed");
            } else {
                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                    .expect("initial commit should succeed");
            }
        }

        (temp_dir, repo)
    }

    fn commit_files(repo: &Repository, root: &Path, message: &str, files: &[(&str, &str)]) {
        for (relative_path, content) in files {
            let file_path = root.join(relative_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent).expect("parent directory should be created");
            }
            std::fs::write(file_path, content).expect("file write should succeed");
        }

        let mut git_index = repo.index().expect("index should be available");
        for (relative_path, _) in files {
            git_index
                .add_path(Path::new(relative_path))
                .expect("path should be added to index");
        }
        git_index.write().expect("index write should succeed");

        let tree_id = git_index.write_tree().expect("tree id should be created");
        let tree = repo.find_tree(tree_id).expect("tree should be found");
        let signature =
            Signature::now("Test User", "test@example.com").expect("signature should exist");

        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        if let Some(parent_commit) = parent {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[&parent_commit],
            )
            .expect("commit should succeed");
        } else {
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                .expect("initial commit should succeed");
        }
    }

    fn commit_empty(repo: &Repository, message: &str) {
        let signature =
            Signature::now("Test User", "test@example.com").expect("signature should exist");
        let parent_commit = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok())
            .expect("parent commit should exist");
        let tree = parent_commit.tree().expect("parent tree should exist");

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )
        .expect("empty commit should succeed");
    }

    fn delete_and_commit_files(repo: &Repository, root: &Path, message: &str, files: &[&str]) {
        for relative_path in files {
            let file_path = root.join(relative_path);
            if file_path.exists() {
                std::fs::remove_file(&file_path).expect("file should be removed");
            }
        }

        let mut git_index = repo.index().expect("index should be available");
        for relative_path in files {
            git_index
                .remove_path(Path::new(relative_path))
                .expect("path should be removed from index");
        }
        git_index.write().expect("index write should succeed");

        let tree_id = git_index.write_tree().expect("tree id should be created");
        let tree = repo.find_tree(tree_id).expect("tree should be found");
        let signature =
            Signature::now("Test User", "test@example.com").expect("signature should exist");

        let parent_commit = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok())
            .expect("parent commit should exist");

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent_commit],
        )
        .expect("delete commit should succeed");
    }

    fn checkout_branch(repo: &Repository, branch_name: &str) {
        let reference_name = format!("refs/heads/{}", branch_name);
        let object = repo
            .revparse_single(&reference_name)
            .expect("branch object should exist");
        repo.checkout_tree(&object, None)
            .expect("checkout tree should succeed");
        repo.set_head(&reference_name)
            .expect("set head should succeed");
    }
}
