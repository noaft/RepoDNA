# RepoDNA

RepoDNA is a local-first memory layer for coding tools.

It builds a SQLite-backed graph from a repository, lets tools search that graph through MCP/API, and stores durable node context so future Codex, Claude, editor, or review sessions do not have to rediscover the same repo knowledge again.

RepoDNA is not another assistant. It is the memory substrate underneath assistants.

## What It Does

- Builds graph nodes for files, directories, Rust functions, structs, traits/interfaces, globals, and future code entities.
- Adds non-Rust files as file nodes instead of trying to parse them as code.
- Stores durable summaries on graph nodes so future sessions can reuse repo knowledge.
- Preserves saved node context across graph rebuilds.
- Tracks source hashes for saved context so stale memory can be detected after source changes.
- Exposes repository memory through a local graph API and an MCP server.

## Quick Start

Pick the repository you want RepoDNA to remember:

```powershell
$env:TARGET_REPO="C:\path\to\your-repo"
```

Build the RepoDNA CLI from this checkout:

```powershell
cargo install --path .
```

Run the quickstart setup:

```powershell
RepoDNA setup $env:TARGET_REPO
```

That one command:

- discovers the target repository
- creates the graph database when it does not exist
- uses repo-local storage by default at `.repodna/graph.db`
- registers the RepoDNA MCP server with Codex

Preview what setup will register:

```powershell
RepoDNA setup $env:TARGET_REPO --print-only
```

Force a rebuild:

```powershell
RepoDNA setup $env:TARGET_REPO --force-build
```

Use a custom MCP server name when you have multiple repos:

```powershell
RepoDNA setup $env:TARGET_REPO --name my_repo_memory
```

If you do not want to install yet, run the same setup command through Cargo:

```powershell
cargo run -- setup $env:TARGET_REPO
```

If you are currently inside another repository and want to run RepoDNA from its source checkout:

```powershell
$env:REPODNA_DIR="C:\path\to\RepoDNA"
$env:TARGET_REPO=(Get-Location).Path

cargo run --manifest-path "$env:REPODNA_DIR\Cargo.toml" -- setup $env:TARGET_REPO
```

## Useful Commands

Most users only need `setup`.

```powershell
RepoDNA setup $env:TARGET_REPO
```

Useful options:

- `--print-only`: show the Codex MCP command without running it.
- `--force-build`: rebuild the repo graph before setup.
- `--name my_repo_memory`: use a custom MCP server name.

## License

MIT. See [LICENSE](LICENSE).
