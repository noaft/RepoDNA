#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCli {
    command: Option<String>,
    command_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexMcpAddRequest {
    pub repo_path: String,
    pub execute: bool,
    pub server_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupRequest {
    pub repo_path: String,
    pub server_name: String,
    pub force_build: bool,
    pub no_build: bool,
    pub print_only: bool,
}

impl ParsedCli {
    pub fn command_name(&self) -> Option<&str> {
        self.command.as_deref()
    }

    pub fn command_arg(&self, index: usize) -> Option<&str> {
        self.command_args.get(index).map(String::as_str)
    }

    pub fn repo_path(&self) -> Option<&str> {
        self.command_arg(0)
    }

    pub fn command_args(&self) -> &[String] {
        &self.command_args
    }

    pub fn parse_codex_mcp_add(&self) -> Result<Option<CodexMcpAddRequest>, String> {
        if self.command_name() != Some("mcp") {
            return Ok(None);
        }

        if self.command_args.get(0).map(String::as_str) != Some("codex")
            || self.command_args.get(1).map(String::as_str) != Some("add")
        {
            return Err(
                "expected `repodna mcp codex add <repo> [--execute] [--name <server-name>]`"
                    .to_string(),
            );
        }

        let mut repo_path = None;
        let mut execute = false;
        let mut server_name = "repo_dna".to_string();
        let mut index = 2;

        while let Some(arg) = self.command_args.get(index) {
            match arg.as_str() {
                "--execute" => {
                    execute = true;
                    index += 1;
                }
                "--name" => {
                    let Some(name) = self.command_args.get(index + 1) else {
                        return Err("--name requires a server name".to_string());
                    };
                    server_name = name.clone();
                    index += 2;
                }
                value if value.starts_with("--") => {
                    return Err(format!("unknown option `{value}`"));
                }
                value => {
                    if repo_path.is_some() {
                        return Err("expected only one repository path".to_string());
                    }
                    repo_path = Some(value.to_string());
                    index += 1;
                }
            }
        }

        Ok(Some(CodexMcpAddRequest {
            repo_path: repo_path.unwrap_or_else(|| ".".to_string()),
            execute,
            server_name,
        }))
    }

    pub fn parse_setup(&self) -> Result<Option<SetupRequest>, String> {
        if self.command_name() != Some("setup") {
            return Ok(None);
        }

        let mut repo_path = None;
        let mut server_name = "repo_dna".to_string();
        let mut force_build = false;
        let mut no_build = false;
        let mut print_only = false;
        let mut index = 0;

        while let Some(arg) = self.command_args.get(index) {
            match arg.as_str() {
                "--name" => {
                    let Some(name) = self.command_args.get(index + 1) else {
                        return Err("--name requires a server name".to_string());
                    };
                    server_name = name.clone();
                    index += 2;
                }
                "--force-build" => {
                    force_build = true;
                    index += 1;
                }
                "--no-build" => {
                    no_build = true;
                    index += 1;
                }
                "--print-only" => {
                    print_only = true;
                    index += 1;
                }
                value if value.starts_with("--") => {
                    return Err(format!("unknown option `{value}`"));
                }
                value => {
                    if repo_path.is_some() {
                        return Err("expected only one repository path".to_string());
                    }
                    repo_path = Some(value.to_string());
                    index += 1;
                }
            }
        }

        if force_build && no_build {
            return Err("--force-build cannot be used with --no-build".to_string());
        }

        Ok(Some(SetupRequest {
            repo_path: repo_path.unwrap_or_else(|| ".".to_string()),
            server_name,
            force_build,
            no_build,
            print_only,
        }))
    }
}

pub fn parse_cli_args<I, S>(args: I) -> Result<ParsedCli, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut parts = args.into_iter();
    let _binary = parts.next();
    let command = parts
        .next()
        .map(|value| canonical_command_name(value.as_ref()).to_string());
    let command_args = parts.map(|value| value.as_ref().to_string()).collect();

    Ok(ParsedCli {
        command,
        command_args,
    })
}

fn canonical_command_name(command: &str) -> &str {
    match command {
        "serve" => "serve-graph",
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexMcpCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl CodexMcpCommand {
    pub fn render(&self) -> String {
        let mut parts = vec![quote_shell_arg(&self.program)];
        parts.extend(self.args.iter().map(|arg| quote_shell_arg(arg)));
        parts.join(" ")
    }
}

pub fn build_codex_mcp_add_command(
    server_name: &str,
    repo_path: &str,
    mcp_program: &str,
    env: impl IntoIterator<Item = (String, String)>,
) -> CodexMcpCommand {
    let mut args = vec![
        "mcp".to_string(),
        "add".to_string(),
        server_name.to_string(),
    ];
    let env = env.into_iter().collect::<Vec<_>>();

    for (key, value) in &env {
        args.push("--env".to_string());
        args.push(format!("{key}={value}"));
    }

    args.push("--".to_string());
    args.push(mcp_program.to_string());
    args.push(repo_path.to_string());

    CodexMcpCommand {
        program: "codex".to_string(),
        args,
        env,
    }
}

fn quote_shell_arg(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '&' | '|' | '<' | '>' | ';'))
    {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_serve_alias_matches_serve_graph() {
        let parsed = parse_cli_args(["repodna", "serve", "C:/repo", "127.0.0.1:3000"]).unwrap();

        assert_eq!(parsed.command_name(), Some("serve-graph"));
    }

    #[test]
    fn parse_build_accepts_target_repo() {
        let parsed = parse_cli_args(["repodna", "build", "C:/repo"]).unwrap();

        assert_eq!(parsed.repo_path(), Some("C:/repo"));
    }

    #[test]
    fn parse_codex_mcp_add_accepts_repo_and_execute_flag() {
        let parsed = parse_cli_args([
            "repodna",
            "mcp",
            "codex",
            "add",
            "C:/Repos/My App",
            "--execute",
            "--name",
            "my_memory",
        ])
        .unwrap();

        let request = parsed.parse_codex_mcp_add().unwrap().unwrap();

        assert_eq!(request.repo_path, "C:/Repos/My App");
        assert_eq!(request.server_name, "my_memory");
        assert!(request.execute);
    }

    #[test]
    fn parse_setup_defaults_to_current_repo_and_repo_dna_name() {
        let parsed = parse_cli_args(["repodna", "setup"]).unwrap();

        let request = parsed.parse_setup().unwrap().unwrap();

        assert_eq!(request.repo_path, ".");
        assert_eq!(request.server_name, "repo_dna");
        assert!(!request.force_build);
        assert!(!request.no_build);
        assert!(!request.print_only);
    }

    #[test]
    fn parse_setup_accepts_repo_name_and_print_only() {
        let parsed = parse_cli_args([
            "repodna",
            "setup",
            "C:/Repos/My App",
            "--name",
            "my_memory",
            "--force-build",
            "--print-only",
        ])
        .unwrap();

        let request = parsed.parse_setup().unwrap().unwrap();

        assert_eq!(request.repo_path, "C:/Repos/My App");
        assert_eq!(request.server_name, "my_memory");
        assert!(request.force_build);
        assert!(!request.no_build);
        assert!(request.print_only);
    }

    #[test]
    fn parse_setup_rejects_conflicting_build_flags() {
        let parsed =
            parse_cli_args(["repodna", "setup", "C:/repo", "--force-build", "--no-build"]).unwrap();

        let err = parsed.parse_setup().unwrap_err();

        assert!(err.contains("--force-build cannot be used with --no-build"));
    }

    #[test]
    fn codex_mcp_command_quotes_repo_path() {
        let command = build_codex_mcp_add_command(
            "repo_dna",
            "C:/Repos/My App",
            "repodna_mcp",
            [("REPODNA_HOME".to_string(), "C:/Memory Root".to_string())],
        );
        let rendered = command.render();

        assert!(rendered.contains("--env \"REPODNA_HOME=C:/Memory Root\""));
        assert!(rendered.contains("repodna_mcp \"C:/Repos/My App\""));
    }
}
