# RepoDNA

RepoDNA is a local-first memory layer for coding tools.

It builds a SQLite-backed graph from a repository, lets tools search that graph through MCP/API, and stores durable node context so future Codex, Claude, editor, or review sessions do not have to rediscover the same repo knowledge again.

RepoDNA is not another assistant. It is the memory substrate underneath assistants.

## What It Does

- Builds graph nodes for files, directories, Rust functions, structs, traits/interfaces, globals, and future code entities.
- Adds non-Rust files as file nodes instead of trying to parse them as code.
- Stores durable summaries on graph nodes with `add_node_context` / `update_node_description`.
- Preserves saved node context across graph rebuilds.
- Tracks source hashes for saved context so stale memory can be detected after source changes.
- Exposes repository memory through a local graph API and an MCP server.

## Quick Start

Build the graph:

```powershell
$env:TARGET_REPO="C:\path\to\your-repo"

cargo run -- build $env:TARGET_REPO
```

Run the MCP server:

```powershell
$env:TARGET_REPO="C:\path\to\your-repo"

cargo run --bin repodna_mcp -- $env:TARGET_REPO
```

Register with Codex:

```powershell
codex mcp add repo_dna -- cargo run --bin repodna_mcp -- $env:TARGET_REPO
```

Optional: run the graph API:

```powershell
$env:TARGET_REPO="C:\path\to\your-repo"

cargo run -- serve $env:TARGET_REPO 127.0.0.1:3000
```

If you are currently inside another repository and want to run RepoDNA from its source checkout:

```powershell
$env:REPODNA_DIR="C:\path\to\RepoDNA"
$env:TARGET_REPO=(Get-Location).Path

cargo run --manifest-path "$env:REPODNA_DIR\Cargo.toml" -- build $env:TARGET_REPO
cargo run --manifest-path "$env:REPODNA_DIR\Cargo.toml" --bin repodna_mcp -- $env:TARGET_REPO
```

## MCP Workflow

When an agent enters a repo:

```text
first_look
-> read recommended nodes
-> add_node_context for nodes it understands
```

When an agent needs to find something:

```text
search_nodes
-> copy results[].node_id
-> read source/docs if summary is missing or weak
-> add_node_context or update_node_description
```

When source has changed:

```text
context_health
-> inspect stale nodes
-> read current source or diff
-> update_node_description with the exact node_id
```

Important: agents should not invent node ids. `node_id` is a handle copied exactly from `first_look`, `context_health`, or `search_nodes`.

## MCP Tools

- `first_look`: gives a bootstrap path for a new or unfamiliar repo.
- `context_health`: reports missing, stale, deleted, or unknown node context.
- `search_nodes`: searches graph nodes using the same SQLite FTS/BM25 index as the graph viewer.
- `add_node_context`: saves durable context for a node after reading source/docs.
- `update_node_description`: replaces stale or wrong node context after reading current source/docs/diff.

## Storage

Default behavior:

```text
<target-repo>/.repodna/graph.db
<target-repo>/.repodna/state.json
```

This keeps build, API, and MCP flows pointed at the same local repository memory without requiring environment variables.

Optional shared storage:

```powershell
$env:REPODNA_HOME="$env:LOCALAPPDATA\RepoDNA"
```

With `REPODNA_HOME`, each repository gets its own graph database under the shared RepoDNA home. If you register MCP with a shared home, pass the same env to the MCP command:

```powershell
codex mcp add repo_dna --env REPODNA_HOME="$env:REPODNA_HOME" -- cargo run --bin repodna_mcp -- $env:TARGET_REPO
```

Use `REPODNA_DB_PATH` only when you want to pin RepoDNA to one explicit SQLite file:

```powershell
$env:REPODNA_DB_PATH="C:\path\to\repo-a\graph.db"
$env:TARGET_REPO="C:\path\to\repo-a"

cargo run -- build $env:TARGET_REPO
```

## Embeddings

Saved node context is embedded for retrieval. Defaults are local-first.

OpenAI-compatible backend:

```powershell
$env:REPODNA_EMBEDDING_PROVIDER='openai'
$env:REPODNA_EMBEDDING_MODEL='text-embedding-3-small'
$env:OPENAI_API_KEY='sk-...'
```

Optional local compatible server:

```powershell
$env:OPENAI_BASE_URL='http://localhost:11434/v1'
```

## Development

Check the code:

```powershell
cargo check
```

Run MCP tests:

```powershell
cargo test --bin repodna_mcp
```

Build this repo's graph:

```powershell
$env:TARGET_REPO=(Get-Location).Path
cargo run -- build $env:TARGET_REPO
```

## Product Direction

RepoDNA is built around one idea:

> Code tools should remember what the repository already knows.

The longer roadmap lives in [docs/VISION.md](docs/VISION.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

## License

MIT. See [LICENSE](LICENSE).
