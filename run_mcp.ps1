$ErrorActionPreference = "Stop"

cargo run --bin repodna_mcp -- build $RepoPath
exit $LASTEXITCODE
