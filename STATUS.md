# RepoDNA Status

This file tracks the current implementation status of RepoDNA in this repository.

## Current Storage Layout

Per target repository, RepoDNA now uses:

- `.repodna/graph.db`
- `.repodna/state.json`

`state.json` currently stores:

```json
{
  "last_built_commit": "<head_sha>"
}
```

This is a minimal checkpoint only. It is not yet branch-aware.

## Implemented Features

### 1. Base Graph Storage

SQLite graph storage is implemented in:

- [src/ingestion/mod.rs](/d:/Git/RepoDNA/src/ingestion/mod.rs)

Current node types:

- `Commit`
- `Author`
- `File`
- `Directory`
- `Function`
- `Struct`
- `Interface`
- `Class`
- `GlobalVariable`

Current important edge types:

- `AUTHORED_BY`
- `MODIFIES`
- `MODIFIED`
- `CONTAINS`
- `CALLS`
- `CO_CHANGE`

### 2. Commit -> Function Mapping

Implemented in:

- [src/ingestion/mod.rs](/d:/Git/RepoDNA/src/ingestion/mod.rs)

Behavior:

- Uses git diff between parent commit and current commit
- Extracts changed line ranges
- Matches changed ranges against function spans
- Persists:

```text
Commit --MODIFIED--> Function
```

This is stored permanently in SQLite.

### 3. Distinct Function Identity

Function identity is no longer just `file + name`.

Current function node id is based on:

- file path
- function name
- start line

This avoids collisions when same-name functions exist in the same file.

Related metadata includes:

- `file`
- `symbol_key`
- `start_line`
- `end_line`
- `is_active`
- `deleted`
- `delete`

### 4. Deleted Node Tracking

Implemented for:

- `File`
- `Function`
- `Directory`

Deleted entities are not removed from the graph. They are kept as historical nodes and marked with:

```json
{
  "is_active": false,
  "deleted": true,
  "delete": true
}
```

### 5. Viewer Rendering

Viewer file:

- [docs/GRAPH_VIEWER.html](/d:/Git/RepoDNA/docs/GRAPH_VIEWER.html)

Current behavior:

- deleted nodes render gray
- function labels show file context
- file labels show parent directory context
- selection panel shows metadata and function change history

### 6. Function Insights API

Implemented in:

- [src/api/mod.rs](/d:/Git/RepoDNA/src/api/mod.rs)

Current function-related API support includes:

- commits by function
- functions by commit
- modifying authors for a function
- function diff hunks for recent linked commits

### 7. Scripts

Current helper scripts:

- [run_graph.ps1](/d:/Git/RepoDNA/run_graph.ps1)
- [run_graph.sh](/d:/Git/RepoDNA/run_graph.sh)

PowerShell is the practical Windows entrypoint.

## Current CLI Behavior

Build graph:

```powershell
cargo run -- build .
```

Serve graph API:

```powershell
cargo run -- serve-graph . 127.0.0.1:3000
```

On build:

- `.repodna/graph.db` is created or updated
- `.repodna/state.json` is updated with the current HEAD commit

## Tests

Tests currently cover:

- commit/file/author graph basics
- function extraction
- function call edges
- cross-file calls
- file co-change counts
- single function modification
- multiple function modifications
- whole-file rewrite
- renamed function
- no-op commit
- deleted function retained as inactive
- deleted file retained as inactive
- deleted directory retained as inactive
- same-name functions kept distinct in DB
- `.repodna/state.json` written after build

## Known Limitations

### 1. Incremental Update Is Not Finished

Current build still walks history from HEAD and relies on duplicate-safe inserts plus commit metadata hints.

What exists:

- `function_modifications_indexed` metadata per commit
- `last_built_commit` in `.repodna/state.json`

What is missing:

- dedicated `update` command
- branch-aware checkpointing
- nearest indexed ancestor fallback
- proper incremental range build `A..B`

### 2. Branch Switching Is Not Yet Solved

Current `.repodna/state.json` stores only one `last_built_commit`.

Needed next:

- per-branch state
- ancestor validation
- fallback to nearest indexed commit already present in DB

### 3. Source Parsing Is Heuristic

Current extraction is regex/braces-based, not full tree-sitter.

This works for current scope but is still approximate.

### 4. Old Graph Data May Need Rebuild

After identity or metadata model changes, old graph data may remain in `.repodna/graph.db`.

If graph output looks inconsistent, rebuild from a clean DB may still be necessary.

## Recommended Next Features

Suggested next implementation order:

1. Add `cargo run -- update .`
2. Make `.repodna/state.json` branch-aware
3. Use nearest indexed ancestor instead of full fallback
4. Add viewer filters for deleted nodes
5. Replace regex-based source parsing with stronger structural parsing
