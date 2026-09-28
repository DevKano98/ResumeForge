use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepository {
    pub id: String,
    pub name: String,
    pub name_with_owner: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub url: String,
    pub homepage_url: Option<String>,
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RepositoryRow {
    pub id: i64,
    pub github_repo_id: String,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    pub homepage_url: Option<String>,
    pub visibility: String,
    pub default_branch: String,
    pub latest_commit_sha: Option<String>,
    pub local_clone_path: Option<String>,
    pub pushed_at: Option<String>,
    pub indexed_at: Option<String>,
    pub last_secret_scan_at: Option<String>,
    pub enabled: i64,
}
