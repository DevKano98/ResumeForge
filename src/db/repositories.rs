use crate::models::github::{GitHubRepository, RepositoryRow};
use sqlx::SqlitePool;

const COLUMNS: &str = "id, github_repo_id, owner, name, description, url, homepage_url, visibility, default_branch, latest_commit_sha, local_clone_path, pushed_at, indexed_at, last_secret_scan_at, enabled";

pub async fn upsert_metadata(
    pool: &SqlitePool,
    repo: &GitHubRepository,
) -> Result<i64, sqlx::Error> {
    let owner = repo.name_with_owner.split('/').next().unwrap_or_default();
    let visibility = if repo.is_private { "private" } else { "public" };
    sqlx::query_scalar(
        "INSERT INTO repositories (github_repo_id, owner, name, description, url, homepage_url, visibility, pushed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(github_repo_id) DO UPDATE SET owner=excluded.owner, name=excluded.name,
         description=excluded.description, url=excluded.url, homepage_url=excluded.homepage_url,
         visibility=excluded.visibility, pushed_at=excluded.pushed_at RETURNING id"
    ).bind(&repo.id).bind(owner).bind(&repo.name).bind(&repo.description)
        .bind(&repo.url).bind(&repo.homepage_url).bind(visibility).bind(&repo.pushed_at)
        .fetch_one(pool).await
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<RepositoryRow>, sqlx::Error> {
    sqlx::query_as::<_, RepositoryRow>(&format!("SELECT {COLUMNS} FROM repositories WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<RepositoryRow>, sqlx::Error> {
    sqlx::query_as::<_, RepositoryRow>(&format!(
        "SELECT {COLUMNS} FROM repositories ORDER BY pushed_at DESC, id DESC"
    ))
    .fetch_all(pool)
    .await
}

pub async fn mark_indexed(
    pool: &SqlitePool,
    id: i64,
    sha: &str,
    path: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE repositories SET latest_commit_sha=?, local_clone_path=?, indexed_at=datetime('now'), last_secret_scan_at=datetime('now') WHERE id=?")
        .bind(sha).bind(path).bind(id).execute(pool).await?;
    Ok(())
}
