use git2::{Commit, DiffFormat, DiffOptions, Repository, Revwalk, Sort};

/// Parse repository path from CLI arguments.
///
/// Behavior:
/// - If a path is provided as the first argument after the binary name, use it.
/// - Otherwise, default to the current directory (`.`).
pub fn parse_repo_path<I>(args: I) -> String
where
    I: IntoIterator<Item = String>,
{
    args.into_iter().nth(1).unwrap_or_else(|| ".".to_string())
}

/// Run the full git history scan workflow and print each output line.
///
/// This function acts as an orchestration layer:
/// 1. Collect commit lines from repository history.
/// 2. Print each line to stdout.
pub fn scan_git_history(repo_path: &str) -> Result<(), git2::Error> {
    let lines = collect_commit_lines(repo_path)?;
    let diff_output = collect_commit_diffs(repo_path)?;

    for line in lines {
        println!("{}", line);
    }

    println!("\n=== Repository Diff ===");
    println!("{}", diff_output);

    Ok(())
}

/// Collect all rendered lines for repository history.
///
/// Output format:
/// - First line: scan header with repository path.
/// - Following lines: one formatted entry per commit.
pub fn collect_commit_lines(repo_path: &str) -> Result<Vec<String>, git2::Error> {
    let repo = open_repository(repo_path)?;
    let revwalk = create_revwalk(&repo)?;
    let mut lines = Vec::new();

    lines.push(format!("Scanning git history: {}", repo.path().display()));

    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        lines.push(format_commit_line(&commit));
    }

    Ok(lines)
}

/// Discover and open a git repository from the provided path.
///
/// `Repository::discover` walks parent directories, which means users can pass
/// either the repository root or a nested path inside that repository.
pub fn open_repository(repo_path: &str) -> Result<Repository, git2::Error> {
    Repository::discover(repo_path)
}

/// Build and configure commit traversal order.
///
/// Current strategy:
/// - Start from HEAD.
/// - Sort by commit time (newer commits first).
pub fn create_revwalk<'repo>(repo: &'repo Repository) -> Result<Revwalk<'repo>, git2::Error> {
    let mut revwalk = repo.revwalk()?;

    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TIME)?;

    Ok(revwalk)
}

/// Format a single commit into a display line.
///
/// Fields included:
/// - Commit SHA
/// - Author name and email
/// - Commit timestamp (unix seconds)
/// - Commit summary message
pub fn format_commit_line(commit: &Commit<'_>) -> String {
    let summary = commit.summary().unwrap_or("<no message>");
    let author = commit.author();
    let author_name = author.name().unwrap_or("<unknown>");
    let author_email = author.email().unwrap_or("<unknown>");
    let time = commit.time();

    format!(
        "{} | {} <{}> | {} | {}",
        commit.id(),
        author_name,
        author_email,
        time.seconds(),
        summary
    )
}

/// Collect textual git diffs for each commit in history.
///
/// Each section is rendered as: `commit_sha | summary` followed by patch text.
/// This allows users to inspect change intent commit-by-commit.
pub fn collect_commit_diffs(repo_path: &str) -> Result<String, git2::Error> {
    let repo = open_repository(repo_path)?;
    collect_commit_diffs_from_repository(&repo)
}

/// Build a combined patch string by traversing commit history.
///
/// For non-root commits, the patch is `parent_tree -> commit_tree`.
/// For root commits (no parent), the patch is `empty_tree -> commit_tree`.
fn collect_commit_diffs_from_repository(repo: &Repository) -> Result<String, git2::Error> {
    let mut revwalk = create_revwalk(repo)?;
    let mut output = String::new();

    for oid_result in revwalk.by_ref() {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let commit_tree = commit.tree()?;

        let mut diff_options = DiffOptions::new();
        let diff = if commit.parent_count() > 0 {
            let parent = commit.parent(0)?;
            let parent_tree = parent.tree()?;
            repo.diff_tree_to_tree(
                Some(&parent_tree),
                Some(&commit_tree),
                Some(&mut diff_options),
            )?
        } else {
            repo.diff_tree_to_tree(None, Some(&commit_tree), Some(&mut diff_options))?
        };

        let summary = commit.summary().unwrap_or("<no message>");
        output.push_str(&format!("commit {} | {}\n", commit.id(), summary));

        let mut patch = String::new();
        diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
            patch.push_str(&String::from_utf8_lossy(line.content()));
            true
        })?;

        if patch.trim().is_empty() {
            output.push_str("(no textual patch)\n\n");
        } else {
            output.push_str(&patch);
            if !patch.ends_with('\n') {
                output.push('\n');
            }
            output.push('\n');
        }
    }

    if output.trim().is_empty() {
        Ok("No commits found.".to_string())
    } else {
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn parse_repo_path_uses_current_directory_by_default() {
        let args = vec!["repodna".to_string()];
        let repo_path = parse_repo_path(args);
        assert_eq!(repo_path, ".");
    }

    #[test]
    fn parse_repo_path_uses_provided_argument() {
        let args = vec!["repodna".to_string(), "./demo-repo".to_string()];
        let repo_path = parse_repo_path(args);
        assert_eq!(repo_path, "./demo-repo");
    }

    #[test]
    fn open_repository_discovers_git_repo() {
        let (temp_dir, _) = init_repo_with_commits(&["initial commit"]);
        let repo = open_repository(temp_dir.path().to_str().expect("valid utf8 path"));
        assert!(repo.is_ok());
    }

    #[test]
    fn create_revwalk_returns_commit_entries() {
        let (_temp_dir, repo) = init_repo_with_commits(&["initial commit", "second commit"]);
        let revwalk = create_revwalk(&repo).expect("revwalk should be created");
        let count = revwalk.filter_map(Result::ok).count();
        assert!(count >= 2);
    }

    #[test]
    fn format_commit_line_contains_author_and_message() {
        let (_temp_dir, repo) = init_repo_with_commits(&["format test commit"]);
        let head_commit = get_head_commit(&repo);
        let line = format_commit_line(&head_commit);

        assert!(line.contains("Test User <test@example.com>"));
        assert!(line.contains("format test commit"));
    }

    #[test]
    fn collect_commit_lines_includes_header_and_commit_data() {
        let (temp_dir, _) = init_repo_with_commits(&["first", "second"]);
        let lines = collect_commit_lines(temp_dir.path().to_str().expect("valid utf8 path"))
            .expect("history should be collected");

        assert!(!lines.is_empty());
        assert!(lines[0].starts_with("Scanning git history:"));
        assert!(
            lines
                .iter()
                .any(|line| line.contains("first") || line.contains("second"))
        );
    }

    #[test]
    fn collect_commit_diffs_contains_commit_headers() {
        let (temp_dir, _) = init_repo_with_commits(&["first", "second"]);
        let diff_output = collect_commit_diffs(temp_dir.path().to_str().expect("valid utf8 path"))
            .expect("diff output should be collected");

        assert!(diff_output.contains("commit "));
        assert!(diff_output.contains("first") || diff_output.contains("second"));
    }

    #[test]
    fn collect_commit_diffs_contains_patch_content() {
        let (temp_dir, _) = init_repo_with_commits(&["initial"]);
        let diff_output = collect_commit_diffs(temp_dir.path().to_str().expect("valid utf8 path"))
            .expect("diff output should be collected");

        assert!(diff_output.contains("history.txt"));
        assert!(diff_output.contains("content-0-initial"));
    }

    /// Build a temporary git repository with deterministic commits for testing.
    fn init_repo_with_commits(messages: &[&str]) -> (TempDir, Repository) {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let repo = Repository::init(temp_dir.path()).expect("repo should be initialized");

        for (index, message) in messages.iter().enumerate() {
            let file_content = format!("content-{}-{}", index, message);
            std::fs::write(temp_dir.path().join("history.txt"), file_content)
                .expect("file write should succeed");

            let mut git_index = repo.index().expect("index should be available");
            git_index
                .add_path(Path::new("history.txt"))
                .expect("path should be added to index");
            git_index.write().expect("index write should succeed");

            let tree_id = git_index.write_tree().expect("tree id should be created");
            let tree = repo.find_tree(tree_id).expect("tree should be found");
            let signature =
                Signature::now("Test User", "test@example.com").expect("signature should exist");

            if let Ok(head) = repo.head() {
                let parent = repo
                    .find_commit(head.target().expect("head oid should exist"))
                    .expect("parent commit should be found");

                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[&parent],
                )
                .expect("commit should succeed");
            } else {
                repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                    .expect("initial commit should succeed");
            }
        }

        (temp_dir, repo)
    }

    /// Read the current HEAD commit from a repository.
    fn get_head_commit(repo: &Repository) -> Commit<'_> {
        let oid = repo
            .head()
            .expect("head should exist")
            .target()
            .expect("head oid should exist");
        repo.find_commit(oid).expect("commit should exist")
    }
}
