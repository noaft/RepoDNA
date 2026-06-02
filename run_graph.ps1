param(
    [string]$RepoPath = ".",
    [string]$BindAddr = "127.0.0.1:3000"
)

$ErrorActionPreference = "Stop"

cargo run -- build $RepoPath
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

cargo run -- serve-graph $RepoPath $BindAddr
exit $LASTEXITCODE
