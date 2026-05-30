-- RepoDNA V0.1 SQLite schema
-- Scope: graph storage only (not integrated into application yet)

PRAGMA foreign_keys = ON;

-- =========================
-- Nodes
-- =========================
-- Examples:
-- commit_abc123
-- file_scheduler
-- func_allocate
-- author_alice
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
CREATE INDEX IF NOT EXISTS idx_nodes_name ON nodes(name);

-- =========================
-- Edges
-- =========================
-- Example relation:
-- commit_abc123 --MODIFIES--> file_scheduler
-- func_allocate --CALLS-----> func_evict
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

-- Optional: avoid duplicate edges with same semantic meaning
CREATE UNIQUE INDEX IF NOT EXISTS uq_edges_source_target_relation
ON edges(source, target, relation);

-- =========================
-- Metadata
-- =========================
-- Generic key-value store for computed insights.
-- Ownership engine writes per-file ownership scores here.
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
