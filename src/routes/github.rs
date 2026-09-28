use crate::errors::AppError;
use crate::services::github::{self, GitHubAuthStatus};
use crate::services::github_indexer::{self, SyncReport};
use crate::state::AppState;
use axum::{extract::State, Json};

pub async fn get_status() -> Json<GitHubAuthStatus> {
    Json(github::auth_status().await)
}

pub async fn sync(State(state): State<AppState>) -> Result<Json<SyncReport>, AppError> {
    let status = github::auth_status().await;
    if !status.authenticated {
        return Err(AppError::BadRequest(status.details));
    }
    let username = status.username.ok_or_else(|| {
        AppError::BadRequest("GitHub username was not available from gh auth status".into())
    })?;
    let report = github_indexer::sync_all(&state.db, &state.config, &username)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(report))
}
