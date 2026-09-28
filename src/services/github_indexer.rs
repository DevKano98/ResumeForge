use crate::config::Config;
use crate::db::repositories;
use crate::models::github::GitHubRepository;
use crate::services::{command_runner::run_command, project_search, secret_scanner};
use serde::Serialize;
use sqlx::SqlitePool;
use std::path::Path;

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub discovered: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

async fn command_text(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let output = run_command(program, args).await?;
    if !output.status.success() {
        anyhow::bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

pub async fn list_remote_repositories(username: &str) -> anyhow::Result<Vec<GitHubRepository>> {
    let json = command_text(
        "gh",
        &[
            "repo",
            "list",
            username,
            "--limit",
            "1000",
            "--json",
            "id,name,nameWithOwner,description,isPrivate,url,homepageUrl,pushedAt",
        ],
    )
    .await?;
    Ok(serde_json::from_str(&json)?)
}

/// Sync only app-owned shallow clones. A matching remote HEAD skips every content read and scan.
pub async fn sync_repository(
    pool: &SqlitePool,
    config: &Config,
    repo: &GitHubRepository,
) -> anyhow::Result<bool> {
    let id = repositories::upsert_metadata(pool, repo).await?;
    let clone_path = config.data_dir.join("repo-cache").join(id.to_string());
    let url = &repo.url;
    let remote = command_text("git", &["ls-remote", url, "HEAD"]).await?;
    let remote_sha = remote
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No remote HEAD for {}", repo.name_with_owner))?;
    if clone_path.join(".git").exists() {
        let local = command_text(
            "git",
            &["-C", &clone_path.to_string_lossy(), "rev-parse", "HEAD"],
        )
        .await?;
        let indexed = repositories::get(pool, id)
            .await?
            .and_then(|r| r.latest_commit_sha);
        if local == remote_sha && indexed.as_deref() == Some(remote_sha) {
            return Ok(false);
        }
        command_text(
            "git",
            &[
                "-C",
                &clone_path.to_string_lossy(),
                "fetch",
                "--depth",
                "1",
                "origin",
            ],
        )
        .await?;
        command_text(
            "git",
            &[
                "-C",
                &clone_path.to_string_lossy(),
                "reset",
                "--hard",
                "FETCH_HEAD",
            ],
        )
        .await?;
    } else {
        tokio::fs::create_dir_all(config.data_dir.join("repo-cache")).await?;
        command_text(
            "git",
            &["clone", "--depth", "1", url, &clone_path.to_string_lossy()],
        )
        .await?;
    }
    let actual_sha = command_text(
        "git",
        &["-C", &clone_path.to_string_lossy(), "rev-parse", "HEAD"],
    )
    .await?;
    if actual_sha != remote_sha {
        anyhow::bail!(
            "Clone HEAD differs from remote HEAD for {}",
            repo.name_with_owner
        );
    }
    // Fail closed: no repository content is parsed if gitleaks is unavailable or fails.
    let excluded = secret_scanner::scan_repo(&clone_path, &config.data_dir.join("temp")).await?;
    project_search::index_project(pool, id, repo, &clone_path, &actual_sha, &excluded).await?;
    repositories::mark_indexed(pool, id, &actual_sha, &clone_path.to_string_lossy()).await?;
    Ok(true)
}

pub async fn sync_all(
    pool: &SqlitePool,
    config: &Config,
    username: &str,
) -> anyhow::Result<SyncReport> {
    let repos = list_remote_repositories(username).await?;
    let mut report = SyncReport {
        discovered: repos.len(),
        ..Default::default()
    };
    for repo in &repos {
        match sync_repository(pool, config, repo).await {
            Ok(true) => report.indexed += 1,
            Ok(false) => report.unchanged += 1,
            Err(err) => report
                .errors
                .push(format!("{}: {}", repo.name_with_owner, err)),
        }
    }
    Ok(report)
}

pub fn repo_cache_path(data_dir: &Path, id: i64) -> std::path::PathBuf {
    data_dir.join("repo-cache").join(id.to_string())
}
