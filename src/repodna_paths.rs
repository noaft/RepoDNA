use git2::Repository;
use std::{
    env, fs,
    io::{self, Error, ErrorKind},
    path::{Path, PathBuf},
};

const APP_DIR_NAME: &str = "RepoDNA";
const DB_FILE_NAME: &str = "graph.db";
const STATE_FILE_NAME: &str = "state.json";

pub fn resolve_graph_db_path(repo: &Repository) -> PathBuf {
    if let Some(path) = explicit_db_path() {
        return path;
    }

    resolve_repo_storage_dir(repo).join(DB_FILE_NAME)
}

pub fn resolve_state_path(repo: &Repository) -> PathBuf {
    if let Some(path) = explicit_db_path() {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        return parent.join(STATE_FILE_NAME);
    }

    resolve_repo_storage_dir(repo).join(STATE_FILE_NAME)
}

pub fn ensure_storage_dir(repo: &Repository) -> io::Result<PathBuf> {
    let dir = if let Some(path) = explicit_db_path() {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        resolve_repo_storage_dir(repo)
    };

    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn explicit_db_path() -> Option<PathBuf> {
    let raw = env::var_os("REPODNA_DB_PATH")?;
    let path = PathBuf::from(raw);
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

fn resolve_repo_storage_dir(repo: &Repository) -> PathBuf {
    let repo_root = resolve_repo_root(repo);
    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(slugify)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "repo".to_string());
    let repo_hash = fnv1a_64(repo_root.to_string_lossy().as_bytes());

    resolve_storage_base_dir().join(format!("{}-{:016x}", repo_name, repo_hash))
}

fn resolve_storage_base_dir() -> PathBuf {
    if let Some(home) = env::var_os("REPODNA_HOME") {
        let path = PathBuf::from(home);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local_app_data).join(APP_DIR_NAME);
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(format!(".{}", APP_DIR_NAME.to_ascii_lowercase()));
    }

    PathBuf::from(".repodna")
}

fn resolve_repo_root(repo: &Repository) -> PathBuf {
    repo.workdir()
        .map(Path::to_path_buf)
        .or_else(|| repo.path().parent().map(Path::to_path_buf))
        .and_then(|path| fs::canonicalize(&path).ok().or(Some(path)))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn validate_storage_configuration(repo: &Repository) -> io::Result<()> {
    let db_path = resolve_graph_db_path(repo);
    let state_path = resolve_state_path(repo);

    if db_path.file_name().is_none() || state_path.file_name().is_none() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "resolved RepoDNA storage paths are invalid",
        ));
    }

    Ok(())
}
