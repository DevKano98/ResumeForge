use crate::services::command_runner::run_command;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GitHubAuthStatus {
    pub installed: bool,
    pub authenticated: bool,
    pub username: Option<String>,
    pub scopes: Vec<String>,
    pub details: String,
}

pub fn parse_auth_status(success: bool, output: &str) -> GitHubAuthStatus {
    let username = output.lines().find_map(|line| {
        let marker = "account ";
        let start = line.find(marker)? + marker.len();
        let name = line[start..]
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .next()?;
        (!name.is_empty()).then(|| name.to_string())
    });
    let scopes = output
        .lines()
        .find_map(|line| {
            let (_, raw) = line.split_once("Token scopes:")?;
            Some(
                raw.split(',')
                    .map(|s| s.trim().trim_matches('\'').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    GitHubAuthStatus {
        installed: true,
        authenticated: success,
        username,
        scopes,
        details: if success {
            "GitHub CLI authenticated"
        } else {
            "Run `gh auth login` to connect GitHub"
        }
        .into(),
    }
}

pub async fn auth_status() -> GitHubAuthStatus {
    match run_command("gh", &["auth", "status"]).await {
        Ok(output) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            parse_auth_status(output.status.success(), &combined)
        }
        Err(err) => GitHubAuthStatus {
            installed: false,
            authenticated: false,
            username: None,
            scopes: vec![],
            details: format!("GitHub CLI unavailable: {err}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::parse_auth_status;

    #[test]
    fn parses_account_and_scopes_without_exposing_token() {
        let sample = "github.com\n  ✓ Logged in to github.com account sample-user (keyring)\n  - Token scopes: 'gist', 'read:org', 'repo'";
        let status = parse_auth_status(true, sample);
        assert_eq!(status.username.as_deref(), Some("sample-user"));
        assert_eq!(status.scopes, ["gist", "read:org", "repo"]);
        assert!(status.authenticated);
    }
}
