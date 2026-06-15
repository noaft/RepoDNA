#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCli {
    command: Option<String>,
    command_args: Vec<String>,
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
}
