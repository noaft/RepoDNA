use git2::{Repository, Sort};
use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Node model for graph storage.
pub struct CommitNode {
    pub id: String,
    pub node_type: String,
    pub name: String,
    pub metadata: String,
}

pub struct AuthorNode {
    pub id: String,
    pub node_type: String,
    pub name: String,
    pub metadata: String,
}

pub struct FileNode {
    pub id: String,
    pub node_type: String,
    pub name: String,
    pub metadata: String,
}

pub struct DirectoryNode {
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
    churn_score: i64,
    level: String,
}

struct FunctionHotspotMetric {
    function_id: String,
    function_name: String,
    file_path: String,
    file_commit_count: i64,
    call_degree: i64,
    churn_score: i64,
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

struct RustSymbolNode {
    id: String,
    node_type: String,
    name: String,
    metadata: String,
}

struct RustFileSnapshot {
    file_path: String,
    content: String,
    symbols: Vec<RustSymbolNode>,
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
            id: symbol_id,
            node_type,
            name: symbol_name.to_string(),
            metadata,
        }
    }
}

impl CommitNode {
    /// Build a commit node from a git commit object.
    pub fn from_git_commit(commit: &git2::Commit<'_>, file_count: usize) -> Self {
        let author = commit.author();
        let metadata = json!({
            "sha": commit.id().to_string(),
            "author": author.name().unwrap_or("<unknown>"),
            "email": author.email().unwrap_or("<unknown>"),
            "timestamp": commit.time().seconds(),
            "file_count": file_count
        })
        .to_string();

        Self {
            id: format!("commit_{}", commit.id()),
            node_type: "Commit".to_string(),
            name: commit.summary().unwrap_or("<no message>").to_string(),
            metadata,
        }
    }
}

impl AuthorNode {
    pub fn from_git_commit(commit: &git2::Commit<'_>) -> Self {
        let author = commit.author();
        let author_name = author.name().unwrap_or("<unknown>");
        let author_email = author.email().unwrap_or("unknown@example.com");

        Self {
            id: format!("author_{}", sanitize_id(author_email)),
            node_type: "Author".to_string(),
            name: author_name.to_string(),
            metadata: json!({
                "name": author_name,
                "email": author_email
            })
            .to_string(),
        }
    }
}

impl FileNode {
    pub fn from_path(path: &str) -> Self {
        Self {
            id: format!("file_{}", sanitize_id(path)),
            node_type: "File".to_string(),
            name: path.to_string(),
            metadata: json!({ "path": path }).to_string(),
        }
    }
}

impl DirectoryNode {
    pub fn from_path(path: &str) -> Self {
        Self {
            id: format!("directory_{}", sanitize_id(path)),
            node_type: "Directory".to_string(),
            name: path.to_string(),
            metadata: json!({ "path": path }).to_string(),
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

    /// Insert node if missing. Existing nodes are kept unchanged.
    pub fn upsert_node(&self, node: &CommitNode) -> rusqlite::Result<bool> {
        self.upsert_node_internal(&node.id, &node.node_type, &node.name, &node.metadata)
    }

    pub fn upsert_author_node(&self, node: &AuthorNode) -> rusqlite::Result<bool> {
        self.upsert_node_internal(&node.id, &node.node_type, &node.name, &node.metadata)
    }

    pub fn upsert_file_node(&self, node: &FileNode) -> rusqlite::Result<bool> {
        self.upsert_node_internal(&node.id, &node.node_type, &node.name, &node.metadata)
    }

    pub fn upsert_directory_node(&self, node: &DirectoryNode) -> rusqlite::Result<bool> {
        self.upsert_node_internal(&node.id, &node.node_type, &node.name, &node.metadata)
    }

    fn upsert_symbol_node(&self, node: &RustSymbolNode) -> rusqlite::Result<bool> {
        self.upsert_node_internal(&node.id, &node.node_type, &node.name, &node.metadata)
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

    pub fn upsert_or_increment_co_change_edge(
        &self,
        source_file_id: &str,
        target_file_id: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO edges (source, target, relation, metadata)
             VALUES (?1, ?2, 'CO_CHANGE', json_object('count', 1))
             ON CONFLICT(source, target, relation)
             DO UPDATE SET metadata = json_set(
                 COALESCE(edges.metadata, '{}'),
                 '$.count',
                 COALESCE(json_extract(edges.metadata, '$.count'), 0) + 1
             )",
            params![source_file_id, target_file_id],
        )?;

        Ok(())
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

        let mut churn_scores = Vec::<i64>::new();
        let mut raw_rows = Vec::<(String, String, i64)>::new();
        for file in &files {
            let churn_score = self.get_total_commits_for_file(&file.id)?;
            churn_scores.push(churn_score);
            raw_rows.push((file.id.clone(), file.name.clone(), churn_score));
        }

        churn_scores.sort();
        let len = churn_scores.len();
        let low_threshold = churn_scores[(len.saturating_sub(1) * 33) / 100];
        let high_threshold = churn_scores[(len.saturating_sub(1) * 66) / 100];

        let mut metrics = Vec::<FileHotspotMetric>::new();
        for (file_id, file_name, churn_score) in raw_rows {
            let level = if churn_score >= high_threshold {
                "High"
            } else if churn_score >= low_threshold {
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

        let mut churn_scores = Vec::<i64>::new();
        let mut raw_metrics = Vec::<FunctionHotspotMetric>::new();

        for function in &functions {
            let file_path = extract_file_path_from_metadata(&function.metadata);
            let file_commit_count = if file_path.is_empty() {
                0
            } else {
                let file_id = FileNode::from_path(&file_path).id;
                self.get_total_commits_for_file(&file_id)?
            };

            let call_degree = self.get_function_call_degree(&function.id)?;
            let churn_score = file_commit_count * 10 + call_degree;
            churn_scores.push(churn_score);

            raw_metrics.push(FunctionHotspotMetric {
                function_id: function.id.clone(),
                function_name: function.name.clone(),
                file_path,
                file_commit_count,
                call_degree,
                churn_score,
                level: "Low".to_string(),
            });
        }

        churn_scores.sort();
        let len = churn_scores.len();
        let low_threshold = churn_scores[(len.saturating_sub(1) * 33) / 100];
        let high_threshold = churn_scores[(len.saturating_sub(1) * 66) / 100];

        for metric in &mut raw_metrics {
            metric.level = if metric.churn_score >= high_threshold {
                "High"
            } else if metric.churn_score >= low_threshold {
                "Medium"
            } else {
                "Low"
            }
            .to_string();

            let value = json!({
                "function_id": metric.function_id,
                "function": metric.function_name,
                "file": metric.file_path,
                "file_commit_count": metric.file_commit_count,
                "call_degree": metric.call_degree,
                "churn_score": metric.churn_score,
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

    fn upsert_node_internal(
        &self,
        id: &str,
        node_type: &str,
        name: &str,
        metadata: &str,
    ) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO nodes (id, type, name, metadata)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO NOTHING",
            params![id, node_type, name, metadata],
        )?;

        Ok(rows_affected > 0)
    }

    fn ensure_schema(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
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
            ON metadata(key);",
        )?;

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

/// Build the foundation graph from commits reachable from HEAD.
///
/// Behavior:
/// - Uses git2 revwalk from HEAD.
/// - Creates `Commit`, `Author`, and `File` nodes.
/// - Creates `AUTHORED_BY` and `MODIFIES` edges.
/// - Skips duplicates via conflict-safe upserts.
pub fn build_graph(repo_path: &str) -> Result<IngestionReport, Box<dyn std::error::Error>> {
    let repo = Repository::discover(repo_path)?;
    let db_path = resolve_graph_db_path(&repo);
    let repository = CommitRepository::open(&db_path)?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TIME | Sort::TOPOLOGICAL)?;

    let _ = repository.remove_file_to_function_contains_edges()?;

    let mut scanned = 0usize;
    let mut commit_nodes_inserted = 0usize;
    let mut author_nodes_inserted = 0usize;
    let mut file_nodes_inserted = 0usize;
    let mut directory_nodes_inserted = 0usize;
    let mut authored_by_edges_inserted = 0usize;
    let mut modifies_edges_inserted = 0usize;
    let mut contains_edges_inserted = 0usize;
    let mut call_edges_inserted = 0usize;
    let mut co_change_pairs_processed = 0usize;
    let mut function_nodes_inserted = 0usize;
    let mut class_nodes_inserted = 0usize;
    let mut struct_nodes_inserted = 0usize;
    let mut interface_nodes_inserted = 0usize;
    let mut global_variable_nodes_inserted = 0usize;

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let files = collect_modified_files(&repo, &commit)?;

        let commit_node = CommitNode::from_git_commit(&commit, files.len());
        let author_node = AuthorNode::from_git_commit(&commit);

        scanned += 1;
        if repository.upsert_node(&commit_node)? {
            commit_nodes_inserted += 1;
        }

        if repository.upsert_author_node(&author_node)? {
            author_nodes_inserted += 1;
        }

        let authored_by_edge = EdgeRecord {
            source: commit_node.id.clone(),
            target: author_node.id.clone(),
            relation: "AUTHORED_BY".to_string(),
            metadata: Some(json!({ "sha": commit.id().to_string() }).to_string()),
        };

        if repository.upsert_edge(&authored_by_edge)? {
            authored_by_edges_inserted += 1;
        }

        let file_pairs = generate_file_pairs(&files);

        for file_path in files {
            let file_node = FileNode::from_path(&file_path);
            if repository.upsert_file_node(&file_node)? {
                file_nodes_inserted += 1;
            }

            let (directory_nodes, contains_edges) = build_directory_hierarchy(&file_path, &file_node.id);
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

            let modifies_edge = EdgeRecord {
                source: commit_node.id.clone(),
                target: file_node.id,
                relation: "MODIFIES".to_string(),
                metadata: Some(json!({ "path": file_path }).to_string()),
            };

            if repository.upsert_edge(&modifies_edge)? {
                modifies_edges_inserted += 1;
            }
        }

        for (left_file_id, right_file_id) in file_pairs {
            repository.upsert_or_increment_co_change_edge(&left_file_id, &right_file_id)?;
            co_change_pairs_processed += 1;
        }
    }

    if let Some(workdir) = repo.workdir() {
        let rust_files = collect_rust_source_files(workdir)?;
        let mut rust_snapshots = Vec::<RustFileSnapshot>::new();

        for rust_file in rust_files {
            let file_path = rust_file.strip_prefix(workdir).unwrap_or(&rust_file);
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

            let content = fs::read_to_string(&rust_file)?;
            let symbols = extract_rust_symbols(&file_path_str, &content)?;
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
        + co_change_pairs_processed
        + function_nodes_inserted
        + class_nodes_inserted
        + struct_nodes_inserted
        + interface_nodes_inserted
        + global_variable_nodes_inserted;

    let duplicates_skipped = total_possible.saturating_sub(inserted_total);

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

fn collect_modified_files(
    repo: &Repository,
    commit: &git2::Commit<'_>,
) -> Result<Vec<String>, git2::Error> {
    let commit_tree = commit.tree()?;
    let mut options = git2::DiffOptions::new();
    let diff = if commit.parent_count() > 0 {
        let parent = commit.parent(0)?;
        let parent_tree = parent.tree()?;
        repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), Some(&mut options))?
    } else {
        repo.diff_tree_to_tree(None, Some(&commit_tree), Some(&mut options))?
    };

    let mut seen = HashSet::<String>::new();
    for delta in diff.deltas() {
        if let Some(path) = pick_delta_path(&delta) {
            seen.insert(path);
        }
    }

    let mut files: Vec<String> = seen.into_iter().collect();
    files.sort();
    Ok(files)
}

fn generate_file_pairs(files: &[String]) -> Vec<(String, String)> {
    let mut pairs = Vec::<(String, String)>::new();
    for left in 0..files.len() {
        for right in (left + 1)..files.len() {
            let left_id = FileNode::from_path(&files[left]).id;
            let right_id = FileNode::from_path(&files[right]).id;

            if left_id <= right_id {
                pairs.push((left_id, right_id));
            } else {
                pairs.push((right_id, left_id));
            }
        }
    }

    pairs
}

fn pick_delta_path(delta: &git2::DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
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

fn build_function_indexes(
    snapshots: &[RustFileSnapshot],
) -> (HashMap<String, Vec<String>>, HashMap<String, String>) {
    let mut by_name = HashMap::<String, Vec<String>>::new();
    let mut by_file_and_name = HashMap::<String, String>::new();

    for snapshot in snapshots {
        for symbol in &snapshot.symbols {
            if symbol.node_type != "Function" {
                continue;
            }

            by_name
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol.id.clone());

            by_file_and_name.insert(
                function_file_key(&snapshot.file_path, &symbol.name),
                symbol.id.clone(),
            );
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
    function_id_by_file_and_name: &HashMap<String, String>,
) -> Result<Vec<EdgeRecord>, regex::Error> {
    let fn_decl_re = Regex::new(
        r"^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"
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
            if let Some(source_id) = function_id_by_file_and_name.get(&source_key) {
                for caps in call_re.captures_iter(line) {
                    if let Some(name_match) = caps.get(1) {
                        let callee_name = name_match.as_str();
                        let targets = resolve_called_function_ids(callee_name, function_ids_by_name);
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
                                    metadata: Some(json!({
                                        "file": file_path,
                                        "line": line_number,
                                        "callee": callee_name
                                    }).to_string()),
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

fn build_directory_hierarchy(file_path: &str, file_node_id: &str) -> (Vec<DirectoryNode>, Vec<EdgeRecord>) {
    let normalized = file_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();

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

fn collect_rust_source_files(root: &Path) -> io::Result<Vec<PathBuf>> {
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
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
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

fn extract_rust_symbols(file_path: &str, content: &str) -> Result<Vec<RustSymbolNode>, regex::Error> {
    let fn_re = Regex::new(r"^\s*(pub\s+)?(async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let struct_re = Regex::new(r"^\s*(pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let trait_re = Regex::new(r"^\s*(pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let impl_re = Regex::new(r"^\s*impl(?:<[^>]+>)?\s+([A-Za-z_][A-Za-z0-9_]*)")?;
    let global_re = Regex::new(r"^\s*(pub\s+)?(static|const)\s+(mut\s+)?([A-Za-z_][A-Za-z0-9_]*)")?;

    let mut symbols = Vec::<RustSymbolNode>::new();
    let mut seen = HashSet::<String>::new();

    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }

        if let Some(caps) = struct_re.captures(line) {
            if let Some(name_match) = caps.get(2) {
                let symbol = RustSymbolNode::new(RustSymbolKind::Struct, file_path, name_match.as_str(), line_number);
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = trait_re.captures(line) {
            if let Some(name_match) = caps.get(2) {
                let symbol = RustSymbolNode::new(RustSymbolKind::Interface, file_path, name_match.as_str(), line_number);
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = impl_re.captures(line) {
            if let Some(name_match) = caps.get(1) {
                let class_name = format!("impl {}", name_match.as_str());
                let symbol = RustSymbolNode::new(RustSymbolKind::Class, file_path, &class_name, line_number);
                if seen.insert(symbol.id.clone()) {
                    symbols.push(symbol);
                }
            }
        }

        if let Some(caps) = fn_re.captures(line) {
            if let Some(name_match) = caps.get(3) {
                let symbol = RustSymbolNode::new(RustSymbolKind::Function, file_path, name_match.as_str(), line_number);
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

fn resolve_graph_db_path(repo: &Repository) -> PathBuf {
    if let Some(workdir) = repo.workdir() {
        return workdir.join("graph.db");
    }

    if let Some(repo_root) = repo.path().parent() {
        return repo_root.join("graph.db");
    }

    PathBuf::from("graph.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use rusqlite::Connection;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn build_graph_inserts_commits_authors_files_and_edges() {
        let (temp_dir, _repo) = init_repo_with_commits(&["first", "second", "third"]);

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
            .query_row("SELECT COUNT(*) FROM nodes WHERE type = 'File'", [], |row| row.get(0))
            .expect("file count should succeed");
        let directory_count: i64 = db
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
        let contains_count: i64 = db
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

        assert_eq!(report.scanned, 3);
        assert_eq!(commit_count, 3);
        assert_eq!(author_count, 1);
        assert!(file_count >= 1);
        assert!(directory_count >= 1);
        assert_eq!(authored_by_count, 3);
        assert!(modifies_count >= 3);
        assert!(contains_count >= 1);
        assert_eq!(ownership_count, file_count);
        assert_eq!(hotspot_count, file_count);
        assert_eq!(report.ownership_files_computed as i64, file_count);
        assert_eq!(report.hotspot_files_computed as i64, file_count);
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
            .query_row("SELECT COUNT(*) FROM nodes WHERE type = 'Commit'", [], |row| row.get(0))
            .expect("commit count query should succeed");
        let authored_by_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE relation = 'AUTHORED_BY'",
                [],
                |row| row.get(0),
            )
            .expect("edge count query should succeed");

        assert_eq!(first.commit_nodes_inserted, 2);
        assert_eq!(second.commit_nodes_inserted, 0);
        assert_eq!(commit_count, 2);
        assert_eq!(authored_by_count, 2);
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
        let a_id = rust_symbol_id("Function", "src/call_graph.rs", "a");
        let b_id = rust_symbol_id("Function", "src/call_graph.rs", "b");

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
        let main_id = rust_symbol_id("Function", "src/main.rs", "main");
        let run_id = rust_symbol_id("Function", "src/helper.rs", "run");

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
    fn build_graph_computes_file_co_change_counts() {
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
        assert!(report.co_change_pairs_processed >= 4);

        let db = Connection::open(report.db_path).expect("db should open");
        let a_id = FileNode::from_path("src/a.rs").id;
        let b_id = FileNode::from_path("src/b.rs").id;
        let c_id = FileNode::from_path("src/c.rs").id;

        let ab_count: i64 = db
            .query_row(
                "SELECT CAST(COALESCE(json_extract(metadata, '$.count'), 0) AS INTEGER)
                 FROM edges
                 WHERE relation = 'CO_CHANGE' AND source = ?1 AND target = ?2",
                params![a_id, b_id],
                |row| row.get(0),
            )
            .expect("ab co-change should exist");
        let ac_count: i64 = db
            .query_row(
                "SELECT CAST(COALESCE(json_extract(metadata, '$.count'), 0) AS INTEGER)
                 FROM edges
                 WHERE relation = 'CO_CHANGE' AND source = ?1 AND target = ?2",
                params![FileNode::from_path("src/a.rs").id, c_id.clone()],
                |row| row.get(0),
            )
            .expect("ac co-change should exist");
        let bc_count: i64 = db
            .query_row(
                "SELECT CAST(COALESCE(json_extract(metadata, '$.count'), 0) AS INTEGER)
                 FROM edges
                 WHERE relation = 'CO_CHANGE' AND source = ?1 AND target = ?2",
                params![FileNode::from_path("src/b.rs").id, c_id],
                |row| row.get(0),
            )
            .expect("bc co-change should exist");

        assert_eq!(ab_count, 2);
        assert_eq!(ac_count, 1);
        assert_eq!(bc_count, 1);
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

                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[&parent])
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
}
