use crate::{repodna_paths, settings::Settings};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const REGISTRY_FILE_NAME: &str = "repos.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredRepo {
    pub root: String,
    pub db_path: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RepoRegistry {
    repos: Vec<RegisteredRepo>,
}

#[allow(dead_code)]
pub fn register_repo(repo: &Repository) -> io::Result<RegisteredRepo> {
    let root = resolve_repo_root(repo);
    let db_path = repodna_paths::resolve_graph_db_path(repo);
    let entry = RegisteredRepo {
        root: display_path(&root),
        db_path: display_path(&db_path),
    };

    let registry_path = registry_path();
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut registry = read_registry_from_path(&registry_path)?;
    registry.repos.retain(|repo| repo.root != entry.root);
    registry.repos.push(entry.clone());
    registry
        .repos
        .sort_by(|left, right| left.root.cmp(&right.root));
    write_registry_to_path(&registry_path, &registry)?;

    Ok(entry)
}

#[allow(dead_code)]
pub fn registered_repos() -> io::Result<Vec<RegisteredRepo>> {
    Ok(read_registry_from_path(&registry_path())?.repos)
}

fn registry_path() -> PathBuf {
    Settings::from_env().storage_home.join(REGISTRY_FILE_NAME)
}

fn read_registry_from_path(path: &Path) -> io::Result<RepoRegistry> {
    if !path.exists() {
        return Ok(RepoRegistry::default());
    }

    let raw = fs::read_to_string(path)?;
    serde_json::from_str::<RepoRegistry>(&raw)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
}

#[allow(dead_code)]
fn write_registry_to_path(path: &Path, registry: &RepoRegistry) -> io::Result<()> {
    let raw = serde_json::to_string_pretty(registry)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
    fs::write(path, raw)
}

#[allow(dead_code)]
fn resolve_repo_root(repo: &Repository) -> PathBuf {
    repo.workdir()
        .map(Path::to_path_buf)
        .or_else(|| repo.path().parent().map(Path::to_path_buf))
        .and_then(|path| fs::canonicalize(&path).ok().or(Some(path)))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[allow(dead_code)]
fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn registry_upserts_repos_by_root() {
        let dir = TempDir::new().expect("temp dir should be created");
        let registry_path = dir.path().join("repos.json");
        let first = RegisteredRepo {
            root: "C:/repo-a".to_string(),
            db_path: "C:/repo-a/.repodna/graph.db".to_string(),
        };
        let updated = RegisteredRepo {
            root: "C:/repo-a".to_string(),
            db_path: "D:/memory/repo-a.db".to_string(),
        };

        let mut registry = read_registry_from_path(&registry_path).expect("registry should read");
        registry.repos.push(first);
        write_registry_to_path(&registry_path, &registry).expect("registry should write");

        let mut registry = read_registry_from_path(&registry_path).expect("registry should read");
        registry.repos.retain(|repo| repo.root != updated.root);
        registry.repos.push(updated.clone());
        write_registry_to_path(&registry_path, &registry).expect("registry should write");

        let registry = read_registry_from_path(&registry_path).expect("registry should read");

        assert_eq!(registry.repos, vec![updated]);
    }
}
