# RepoDNA: One-Command Repository Memory

RepoDNA is now focused on one simple path: choose a target repository, install the CLI, and run setup.

```powershell
$env:TARGET_REPO="C:\path\to\your-repo"
cargo install --path .
RepoDNA setup $env:TARGET_REPO
```

## Highlights

- Added `RepoDNA setup <repo>` as the main quickstart command.
- Setup builds repository memory when needed.
- Setup registers one global RepoDNA MCP server with Codex automatically.
- Running setup for multiple repositories lets the same MCP server resolve the active git workspace and search the right repo memory.
- Repo memory is stored locally beside the target repository by default.
- README was simplified for users: quickstart first, without internal MCP workflow details.
- Added setup helper options:
  - `--print-only`
  - `--force-build`
  - `--name <server-name>`

## Why It Matters

RepoDNA is becoming a practical memory layer for coding tools.

Instead of rebuilding repository context every new session, tools can connect to a local graph and reuse durable repo knowledge.

This release makes the first run easier: one target repo, one setup command, ready for Codex. For multiple repos, run setup once per repo and keep using the same `repodna` MCP server.
