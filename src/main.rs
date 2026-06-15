use std::env;

mod api;
mod cli_ux;
mod embeddings;
mod history;
mod ingestion;
mod repodna_paths;
mod settings;

/// Entry point for RepoDNA CLI.
///
/// `main` only handles argument parsing and top-level error handling.
/// All domain logic for scanning git history lives in the `history` module.
fn main() {
    let args: Vec<String> = env::args().collect();
    let parsed = match cli_ux::parse_cli_args(&args) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("Invalid arguments: {err}");
            std::process::exit(2);
        }
    };

    if matches!(
        parsed.command_name(),
        Some("build") | Some("ingest-commits")
    ) {
        let repo_path = parsed.repo_path().unwrap_or(".");

        match ingestion::build_graph(repo_path) {
            Ok(report) => {
                println!("Repository graph build completed.");
                println!("Database: {}", report.db_path.display());
                println!("Commits scanned: {}", report.scanned);
                println!("Commit nodes inserted: {}", report.commit_nodes_inserted);
                println!("Author nodes inserted: {}", report.author_nodes_inserted);
                println!("File nodes inserted: {}", report.file_nodes_inserted);
                println!(
                    "Directory nodes inserted: {}",
                    report.directory_nodes_inserted
                );
                println!(
                    "AUTHORED_BY edges inserted: {}",
                    report.authored_by_edges_inserted
                );
                println!(
                    "MODIFIES edges inserted: {}",
                    report.modifies_edges_inserted
                );
                println!(
                    "CONTAINS edges inserted: {}",
                    report.contains_edges_inserted
                );
                println!("CALLS edges inserted: {}", report.call_edges_inserted);
                println!(
                    "MAIN_TREE edges inserted: {}",
                    report.main_tree_edges_inserted
                );
                println!(
                    "MAIN_FLOW edges inserted: {}",
                    report.main_flow_edges_inserted
                );
                println!(
                    "MODIFIED function edges inserted: {}",
                    report.modified_function_edges_inserted
                );
                println!(
                    "CO_CHANGE pairs processed: {}",
                    report.co_change_pairs_processed
                );
                println!(
                    "Function nodes inserted: {}",
                    report.function_nodes_inserted
                );
                println!("Class nodes inserted: {}", report.class_nodes_inserted);
                println!("Struct nodes inserted: {}", report.struct_nodes_inserted);
                println!(
                    "Interface nodes inserted: {}",
                    report.interface_nodes_inserted
                );
                println!(
                    "GlobalVariable nodes inserted: {}",
                    report.global_variable_nodes_inserted
                );
                println!(
                    "Ownership metadata computed for files: {}",
                    report.ownership_files_computed
                );
                println!(
                    "Hotspot metadata computed for files: {}",
                    report.hotspot_files_computed
                );
                println!(
                    "Hotspot metadata computed for functions: {}",
                    report.hotspot_functions_computed
                );
                println!("Duplicates skipped: {}", report.duplicates_skipped);
            }
            Err(err) => {
                eprintln!("Failed to build graph for '{}': {}", repo_path, err);
                std::process::exit(1);
            }
        }
        return;
    }

    if matches!(parsed.command_name(), Some("embed-text")) {
        let text = parsed.command_args().join(" ");

        match embeddings::embed_text(&text) {
            Ok(embedding) => {
                println!(
                    "Embedding generated with {} dimensions.",
                    embedding.vector.len()
                );
                println!("Model: {}", embedding.model);
                println!("Vector length: {}", embedding.vector.len());
            }
            Err(err) => {
                eprintln!("Embedding failed: {err}");
                std::process::exit(1);
            }
        }

        return;
    }

    if matches!(parsed.command_name(), Some("update")) {
        let repo_path = parsed.repo_path().unwrap_or(".");

        match ingestion::update_graph(repo_path) {
            Ok(report) => {
                println!("Repository graph update completed.");
                println!("Database: {}", report.db_path.display());
                println!("Commits scanned: {}", report.scanned);
                println!("Commit nodes inserted: {}", report.commit_nodes_inserted);
                println!("Author nodes inserted: {}", report.author_nodes_inserted);
                println!("File nodes inserted: {}", report.file_nodes_inserted);
                println!(
                    "Directory nodes inserted: {}",
                    report.directory_nodes_inserted
                );
                println!(
                    "AUTHORED_BY edges inserted: {}",
                    report.authored_by_edges_inserted
                );
                println!(
                    "MODIFIES edges inserted: {}",
                    report.modifies_edges_inserted
                );
                println!(
                    "CONTAINS edges inserted: {}",
                    report.contains_edges_inserted
                );
                println!("CALLS edges inserted: {}", report.call_edges_inserted);
                println!(
                    "MAIN_TREE edges inserted: {}",
                    report.main_tree_edges_inserted
                );
                println!(
                    "MAIN_FLOW edges inserted: {}",
                    report.main_flow_edges_inserted
                );
                println!(
                    "MODIFIED function edges inserted: {}",
                    report.modified_function_edges_inserted
                );
                println!(
                    "CO_CHANGE pairs processed: {}",
                    report.co_change_pairs_processed
                );
                println!(
                    "Function nodes inserted: {}",
                    report.function_nodes_inserted
                );
                println!("Class nodes inserted: {}", report.class_nodes_inserted);
                println!("Struct nodes inserted: {}", report.struct_nodes_inserted);
                println!(
                    "Interface nodes inserted: {}",
                    report.interface_nodes_inserted
                );
                println!(
                    "GlobalVariable nodes inserted: {}",
                    report.global_variable_nodes_inserted
                );
                println!(
                    "Ownership metadata computed for files: {}",
                    report.ownership_files_computed
                );
                println!(
                    "Hotspot metadata computed for files: {}",
                    report.hotspot_files_computed
                );
                println!(
                    "Hotspot metadata computed for functions: {}",
                    report.hotspot_functions_computed
                );
                println!("Duplicates skipped: {}", report.duplicates_skipped);
            }
            Err(err) => {
                eprintln!("Failed to update graph for '{}': {}", repo_path, err);
                std::process::exit(1);
            }
        }
        return;
    }

    if matches!(parsed.command_name(), Some("viewer") | Some("serve-graph")) {
        let repo_path = parsed.repo_path().unwrap_or(".");
        let bind_addr = parsed.command_arg(1).unwrap_or("127.0.0.1:3000");

        let repo = match git2::Repository::discover(repo_path) {
            Ok(repo) => repo,
            Err(err) => {
                eprintln!("Failed to discover repository '{}': {}", repo_path, err);
                std::process::exit(1);
            }
        };

        let db_path = repodna_paths::resolve_graph_db_path(&repo);

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
