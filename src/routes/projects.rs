use crate::db::projects;
use crate::errors::AppError;
use crate::models::project::{ProjectDetail, ProjectRow};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<ProjectRow>>, AppError> {
    Ok(Json(
        projects::list(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?,
    ))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProjectDetail>, AppError> {
    let item = projects::get_detail(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("Project {id} not found")))?;
    Ok(Json(item))
}
