use std::env;

mod api;
mod history;
mod ingestion;

/// Entry point for RepoDNA CLI.
///
/// `main` only handles argument parsing and top-level error handling.
/// All domain logic for scanning git history lives in the `history` module.
fn main() {
    let args: Vec<String> = env::args().collect();

    if matches!(args.get(1).map(String::as_str), Some("build"))
        || matches!(args.get(1).map(String::as_str), Some("ingest-commits"))
    {
        let repo_path = args.get(2).map(String::as_str).unwrap_or(".");

        match ingestion::build_graph(repo_path) {
            Ok(report) => {
                println!("Repository graph build completed.");
                println!("Database: {}", report.db_path.display());
                println!("Commits scanned: {}", report.scanned);
                println!("Commit nodes inserted: {}", report.commit_nodes_inserted);
                println!("Author nodes inserted: {}", report.author_nodes_inserted);
                println!("File nodes inserted: {}", report.file_nodes_inserted);
                println!("AUTHORED_BY edges inserted: {}", report.authored_by_edges_inserted);
                println!("MODIFIES edges inserted: {}", report.modifies_edges_inserted);
                println!("Ownership metadata computed for files: {}", report.ownership_files_computed);
                println!("Duplicates skipped: {}", report.duplicates_skipped);
            }
            Err(err) => {
                eprintln!("Failed to build graph for '{}': {}", repo_path, err);
                std::process::exit(1);
            }
        }
        return;
    }

    if matches!(args.get(1).map(String::as_str), Some("viewer"))
        || matches!(args.get(1).map(String::as_str), Some("serve-graph"))
    {
        let repo_path = args.get(2).map(String::as_str).unwrap_or(".");
        let bind_addr = args.get(3).map(String::as_str).unwrap_or("127.0.0.1:3000");

        let repo = match git2::Repository::discover(repo_path) {
            Ok(repo) => repo,
            Err(err) => {
                eprintln!("Failed to discover repository '{}': {}", repo_path, err);
                std::process::exit(1);
            }
        };

        let db_path = if let Some(workdir) = repo.workdir() {
            workdir.join("graph.db")
        } else if let Some(root) = repo.path().parent() {
            root.join("graph.db")
        } else {
            std::path::PathBuf::from("graph.db")
        };

        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                eprintln!("Failed to create async runtime: {}", err);
                std::process::exit(1);
            }
        };

        let result = runtime.block_on(api::serve_graph_api(db_path, bind_addr));
        if let Err(err) = result {
            eprintln!("Graph API server failed: {}", err);
            std::process::exit(1);
        }
        return;
    }

    let repo_path = history::parse_repo_path(args);
    if let Err(err) = history::scan_git_history(&repo_path) {
        eprintln!("Failed to scan git history for '{}': {}", repo_path, err);
        std::process::exit(1);
    }
}
