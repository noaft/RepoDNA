use git2::{Repository, Sort};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use std::collections::HashSet;
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

    pub fn upsert_edge(&self, edge: &EdgeRecord) -> rusqlite::Result<bool> {
        let rows_affected = self.conn.execute(
            "INSERT INTO edges (source, target, relation, metadata)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source, target, relation) DO NOTHING",
            params![edge.source, edge.target, edge.relation, edge.metadata],
        )?;

        Ok(rows_affected > 0)
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

    fn get_total_commits_for_file(&self, file_id: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(DISTINCT source)
             FROM edges
             WHERE relation = 'MODIFIES' AND target = ?1",
            params![file_id],
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
    pub ownership_files_computed: usize,
    pub hotspot_files_computed: usize,
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

    let mut scanned = 0usize;
    let mut commit_nodes_inserted = 0usize;
    let mut author_nodes_inserted = 0usize;
    let mut file_nodes_inserted = 0usize;
    let mut directory_nodes_inserted = 0usize;
    let mut authored_by_edges_inserted = 0usize;
    let mut modifies_edges_inserted = 0usize;
    let mut contains_edges_inserted = 0usize;

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
    }

    let ownership_files_computed = repository.compute_and_store_file_ownership()?;
    let hotspot_files_computed = repository.compute_and_store_file_hotspots()?;

    let total_possible = scanned
        + scanned
        + file_nodes_inserted
        + directory_nodes_inserted
        + authored_by_edges_inserted
        + modifies_edges_inserted
        + contains_edges_inserted;
    let inserted_total = commit_nodes_inserted
        + author_nodes_inserted
        + file_nodes_inserted
        + directory_nodes_inserted
        + authored_by_edges_inserted
        + modifies_edges_inserted
        + contains_edges_inserted;

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
        ownership_files_computed,
        hotspot_files_computed,
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
}
