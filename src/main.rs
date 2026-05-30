use std::env;

mod history;

/// Entry point for RepoDNA CLI.
///
/// `main` only handles argument parsing and top-level error handling.
/// All domain logic for scanning git history lives in the `history` module.
fn main() {
    let repo_path = history::parse_repo_path(env::args());

    if let Err(err) = history::scan_git_history(&repo_path) {
        eprintln!("Failed to scan git history for '{}': {}", repo_path, err);
        std::process::exit(1);
    }
}
