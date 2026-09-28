use crate::services::command_runner::run_command;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub fn is_ignored_path(relative: &Path) -> bool {
    let value = relative
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let parts: Vec<&str> = value.split('/').collect();
    if parts.iter().any(|part| {
        matches!(
            *part,
            ".git" | "node_modules" | "target" | "dist" | "build" | "vendor" | "secrets"
        )
    }) {
        return true;
    }
    let name = parts.last().copied().unwrap_or_default();
    name.starts_with(".env")
        || name.starts_with("credentials.")
        || [".pem", ".key", ".p12", ".crt"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

pub fn contains_secret_pattern(content: &str) -> bool {
    let pattern = Regex::new(r##"(?im)^\s*(?:export\s+)?(?:API[_-]?KEY|SECRET|TOKEN|PASSWORD|PRIVATE[_-]?KEY)\s*[:=]\s*['"]?[^\s'"#]{8,}"##).unwrap();
    pattern.is_match(content)
}

pub async fn scan_repo(clone_path: &Path, temp_dir: &Path) -> anyhow::Result<HashSet<PathBuf>> {
    tokio::fs::create_dir_all(temp_dir).await?;
    let report = temp_dir.join(format!("gitleaks_{}.json", uuid::Uuid::new_v4()));
    let source = clone_path.to_string_lossy();
    let report_arg = report.to_string_lossy();
    let output = run_command(
        "gitleaks",
        &["detect", "--source", &source, "--no-git", "-r", &report_arg],
    )
    .await
    .map_err(|e| anyhow::anyhow!("gitleaks is required before indexing repository content: {e}"))?;
    if !output.status.success() && output.status.code() != Some(1) {
        anyhow::bail!(
            "gitleaks scan failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let bytes = match tokio::fs::read(&report).await {
        Ok(bytes) => bytes,
        Err(_) if output.status.success() => b"[]".to_vec(),
        Err(err) => anyhow::bail!("gitleaks found leaks but did not produce a report: {err}"),
    };
    let _ = tokio::fs::remove_file(&report).await;
    let findings: serde_json::Value = if bytes.is_empty() {
        serde_json::json!([])
    } else {
        serde_json::from_slice(&bytes)?
    };
    let mut excluded = HashSet::new();
    let clone_absolute =
        std::fs::canonicalize(clone_path).unwrap_or_else(|_| clone_path.to_path_buf());
    if let Some(items) = findings.as_array() {
        for item in items {
            if let Some(file) = item.get("File").and_then(|v| v.as_str()) {
                let path = PathBuf::from(file);
                let absolute = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                let relative = absolute
                    .strip_prefix(&clone_absolute)
                    .or_else(|_| path.strip_prefix(clone_path))
                    .unwrap_or(&path);
                excluded.insert(relative.to_path_buf());
            }
        }
    }
    Ok(excluded)
}

#[cfg(test)]
mod tests {
    use super::{contains_secret_pattern, is_ignored_path};
    use std::path::Path;

    #[test]
    fn excludes_secret_files_and_assignment_patterns() {
        assert!(is_ignored_path(Path::new("src/.env.local")));
        assert!(is_ignored_path(Path::new("target/debug/app")));
        assert!(!is_ignored_path(Path::new("src/main.rs")));
        assert!(contains_secret_pattern("API_KEY=supersecretvalue"));
        assert!(!contains_secret_pattern("API_KEY=example"));
    }
}
