# AGENTS.md

This file guides coding agents working in the RepoDNA repository.

## Mission

RepoDNA is building the memory layer for code tools.

The product goal is not "another assistant." The goal is persistent repository memory:

- ingest repository history and structure
- store durable knowledge in a local graph
- expose that knowledge to tools through local APIs and MCP
- reduce repeated rediscovery across sessions

When making changes, optimize for continuity, retrieval quality, and operational simplicity.

## What Matters Most

Agents should preserve these project priorities:

1. Durable repository memory beats short-lived prompt memory.
2. Local-first workflows are preferred over cloud-dependent assumptions.
3. MCP ergonomics matter because RepoDNA is a substrate for other tools.
4. Storage path behavior must stay predictable across CLI, API, and MCP flows.
5. Changes should improve context recovery, not just raw feature count.

## Codebase Map

- [src/main.rs](/abs/path/d:/Git/RepoDNA/src/main.rs:1)
  CLI entry point. Wires commands such as `build`, `update`, `viewer`, and `serve-graph`.
- [src/ingestion/mod.rs](/abs/path/d:/Git/RepoDNA/src/ingestion/mod.rs:1)
  Core graph-building logic. Commits, authors, files, directories, functions, edges, hotspots, ownership, and rebuild/update flow live here.
- [src/api/mod.rs](/abs/path/d:/Git/RepoDNA/src/api/mod.rs:1)
  Graph API server over the persisted database.
- [src/bin/repodna_mcp.rs](/abs/path/d:/Git/RepoDNA/src/bin/repodna_mcp.rs:1)
  MCP server. Right now it exposes graph-backed node search plus function-context memory tools.
- [src/settings.rs](/abs/path/d:/Git/RepoDNA/src/settings.rs:1)
  Central place for RepoDNA environment-driven settings.
- [src/repodna_paths.rs](/abs/path/d:/Git/RepoDNA/src/repodna_paths.rs:1)
  Resolves storage paths for `graph.db` and `state.json`.
- [README.md](/abs/path/d:/Git/RepoDNA/README.md:1)
  Product framing and operator-facing usage.
- [docs/VISION.md](/abs/path/d:/Git/RepoDNA/docs/VISION.md:1)
  Strategic direction. Keep new work aligned with the "memory layer" story.

## Trusted Commands

Use these first when validating changes:

```powershell
cargo check
```

Build the graph for the repo:

```powershell
cargo run -- build D:\Git\RepoDNA
```

Use a fixed shared DB path:

```powershell
$env:REPODNA_DB_PATH='D:\RepoDNA\.repodna\graph.db'
cargo run -- build D:\Git\RepoDNA
```

Run the MCP server:

```powershell
$env:REPODNA_DB_PATH='D:\RepoDNA\.repodna\graph.db'
cargo run --bin repodna_mcp -- D:\Git\RepoDNA
```

Serve the graph API:

```powershell
cargo run -- serve-graph D:\Git\RepoDNA 127.0.0.1:3000
```

## Storage Rules

Storage behavior is a core part of the product. Be careful when changing it.

- `REPODNA_DB_PATH` pins RepoDNA to one concrete SQLite file.
- `REPODNA_HOME` sets the storage root for RepoDNA-managed graph/state files.
- `Settings::from_env()` in [src/settings.rs](/abs/path/d:/Git/RepoDNA/src/settings.rs:1) is the canonical place for env-driven settings.
- Path resolution belongs in [src/repodna_paths.rs](/abs/path/d:/Git/RepoDNA/src/repodna_paths.rs:1), not scattered through the codebase.

If you add a new environment variable:

1. Define it in `src/settings.rs`.
2. Thread it through the relevant path or runtime logic.
3. Update `.env.example` if user-configurable.
4. Update `README.md` if it affects setup or operator behavior.

## MCP Rules

The MCP server must be easy to launch and resilient in local environments.

- Avoid unnecessary startup dependencies before the MCP handshake.
- If a setting already gives a direct DB path, prefer using it instead of rediscovering the repo.
- Remember that MCP failures often look like handshake failures even when the real problem is early process exit.
- Keep MCP tool outputs schema-safe. Root output schemas must be objects, not arrays.

## RepoDNA Memory-First Workflow

When an agent needs to understand repository code, prefer RepoDNA memory before broad filesystem search:

1. Before reading files broadly, rebuilding repository context, running wide text search, or opening many files, ask RepoDNA first.
2. On a new or unfamiliar repository, call `first_look`. If it returns `bootstrap_needed`, read its recommended nodes and then call `add_node_context` for each node you genuinely understand.
3. When returning after code changes, call `context_health`. If it reports stale nodes, treat saved summaries as orientation only, inspect the current source or diff, then call `update_node_description` with the exact `node_id`.
4. Call `search_nodes` for targeted repository discovery. It uses the same SQLite FTS/BM25 node index as the graph viewer search and accepts concrete hints or short search terms: node name, type, id, symbol, file path, directory, or source-tree hint.
5. Treat results as graph landing points: inspect `type`, `name`, `metadata`, `summary`, `bm25_score`, and `relevance` before deciding what to read next.
6. Remember that a node can be a `File`, `Directory`, `Function`, `Struct`, `Interface`, `GlobalVariable`, or future code entity.
7. If a relevant result has a useful non-empty `summary`, use that saved context first.
8. If a relevant node exists but `summary` is empty, stale, or too generic, inspect the source code or docs yourself.
9. After understanding the node, call `add_node_context` with the exact `node_id` and a concise high-level summary of what the node is for.
10. If RepoDNA has no relevant result, fall back to normal code search and reading.

This loop is core product behavior: every fresh investigation should improve durable repository memory for the next session.

If you change MCP behavior:

1. Keep startup quiet on `stdio`.
2. Avoid noisy logging to stdout during handshake.
3. Make failure messages actionable.
4. Re-check that Codex-style `cargo run --bin repodna_mcp -- <repo>` launch still works.

## Ingestion Rules

When modifying graph construction:

- preserve id stability where possible
- avoid breaking existing node or edge semantics casually
- keep incremental update behavior in mind, not just full rebuilds
- prefer additive metadata changes over destructive schema churn

If you change node/edge behavior, think through:

- rebuild impact
- update impact
- MCP/API retrieval impact
- test fixture assumptions in `src/ingestion/mod.rs`

## Product Alignment

Good changes usually improve one of these:

- context persistence
- retrieval usefulness
- graph quality
- operational reliability
- agent ergonomics

Less aligned changes usually:

- add flashy surface area without improving memory
- hardcode repo-specific assumptions
- spread settings logic across many files
- make MCP startup more fragile
- optimize for one session while hurting long-term continuity

## Editing Preferences

- Keep changes local and intention-revealing.
- Prefer small helpers over duplicated path/env logic.
- Preserve current architectural boundaries unless there is a clear simplification.
- Update docs when behavior changes, especially `README.md`, `docs/VISION.md`, and `.env.example`.

## Done Criteria

A change is in good shape when:

- the relevant Rust code compiles with `cargo check`
- storage and env behavior remain coherent
- CLI, API, and MCP paths still agree on where data lives
- docs reflect any operator-facing behavior changes
- the change strengthens RepoDNA as a memory layer for tools

## Quick Heuristic

Before shipping, ask:

> Does this make RepoDNA better at helping tools remember what the repository already knows?

If the answer is no, the change is probably off-center.
