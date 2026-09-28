use crate::db::resumes as db_resumes;
use crate::errors::AppError;
use crate::models::resume::{CreateResumeRequest, DeleteResumeResponse, ResumeRow};
use crate::state::AppState;
use axum::{
    extract::{Path as AxumPath, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};

const MAX_JOB_DESCRIPTION_BYTES: usize = 64 * 1024; // 64 KB
const MAX_EXTRA_INSTRUCTIONS_BYTES: usize = 16 * 1024; // 16 KB

/// POST /api/resumes
/// Creates a queued resume and returns without waiting for compilation.
pub async fn create_resume(
    State(state): State<AppState>,
    Json(payload): Json<CreateResumeRequest>,
) -> Result<(StatusCode, Json<ResumeRow>), AppError> {
    if payload.job_description.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Job description cannot be empty".to_string(),
        ));
    }
    if payload.job_description.len() > MAX_JOB_DESCRIPTION_BYTES {
        return Err(AppError::BadRequest(format!(
            "Job description exceeds maximum allowed size of 64 KB (received {} bytes)",
            payload.job_description.len()
        )));
    }
    if let Some(ref extra) = payload.extra_instructions {
        if extra.len() > MAX_EXTRA_INSTRUCTIONS_BYTES {
            return Err(AppError::BadRequest(format!(
                "Extra instructions exceed maximum allowed size of 16 KB (received {} bytes)",
                extra.len()
            )));
        }
    }

    let inserted = db_resumes::insert_resume(
        &state.db,
        payload.company.as_deref(),
        payload.role.as_deref(),
        &payload.job_description,
        payload.extra_instructions.as_deref(),
        payload.parent_resume_id,
        "queued",
    )
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert resume: {}", e)))?;

    if let Err(err) = state.generation_tx.try_send(inserted.id) {
        sqlx::query("UPDATE resumes SET status = 'cancelled', error_stage = 'generation_queue', error_detail = ?, completed_at = datetime('now') WHERE id = ?")
            .bind(err.to_string())
            .bind(inserted.id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to record queue error: {}", e)))?;
        return Err(AppError::Internal(anyhow::anyhow!(
            "Generation queue unavailable: {}",
            err
        )));
    }

    Ok((StatusCode::CREATED, Json(inserted)))
}

/// GET /api/resumes
/// Lists all resumes ordered newest first.
pub async fn regenerate_resume(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<(StatusCode, Json<ResumeRow>), AppError> {
    let source = db_resumes::get_resume_by_id(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("Resume #{id} not found")))?;
    create_resume(
        State(state),
        Json(CreateResumeRequest {
            company: source.company,
            role: source.role,
            job_description: source.job_description,
            extra_instructions: source.extra_instructions,
            parent_resume_id: Some(id),
        }),
    )
    .await
}

pub async fn list_resumes(State(state): State<AppState>) -> Result<Json<Vec<ResumeRow>>, AppError> {
    let list = db_resumes::list_resumes(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?;
    Ok(Json(list))
}

/// GET /api/resumes/:id
/// Retrieves metadata and status for a single resume.
pub async fn get_resume(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<ResumeRow>, AppError> {
    let row = db_resumes::get_resume_by_id(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Resume #{} not found", id)))?;
    Ok(Json(row))
}

/// DELETE /api/resumes/:id
/// Permanently deletes the database row (cascading to dependent tables) AND deletes data/applications/<id>/ from disk.
pub async fn delete_resume(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<DeleteResumeResponse>, AppError> {
    let row = db_resumes::get_resume_by_id(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?;

    if row.is_none() {
        return Err(AppError::NotFound(format!("Resume #{} not found", id)));
    }

    // 1. Delete DB row (cascades to resume_projects and generation_events via SQLite foreign keys)
    let deleted_from_db = db_resumes::delete_resume_by_id(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to delete resume row: {}", e)))?;
    if !deleted_from_db {
        return Err(AppError::Conflict(
            "Cannot delete a resume while generation is running".to_string(),
        ));
    }

    // 2. Delete application directory from disk
    let app_dir = state
        .config
        .data_dir
        .join("applications")
        .join(id.to_string());

    let mut directory_removed = false;
    if app_dir.exists() {
        tokio::fs::remove_dir_all(&app_dir).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to delete resume directory: {}", e))
        })?;
        directory_removed = true;
    }

    Ok(Json(DeleteResumeResponse {
        success: deleted_from_db,
        deleted_id: id,
        directory_removed,
    }))
}

/// GET /api/resumes/:id/pdf
/// Serves the compiled PDF for a resume application.
pub async fn get_resume_pdf(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<impl IntoResponse, AppError> {
    let pdf_path = state
        .config
        .data_dir
        .join("applications")
        .join(id.to_string())
        .join("resume.pdf");

    if !pdf_path.exists() {
        return Err(AppError::NotFound(format!(
            "PDF not found for resume '{}'",
            id
        )));
    }

    let bytes = tokio::fs::read(&pdf_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read PDF: {}", e)))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/pdf"),
    );
    if let Ok(val) = HeaderValue::from_str(&format!("inline; filename=\"resume_{}.pdf\"", id)) {
        headers.insert(header::CONTENT_DISPOSITION, val);
    }

    Ok((headers, bytes))
}

/// GET /api/resumes/:id/tex
/// Serves the LaTeX source for a resume application.
pub async fn get_resume_tex(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<impl IntoResponse, AppError> {
    let tex_path = state
        .config
        .data_dir
        .join("applications")
        .join(id.to_string())
        .join("resume.tex");

    if !tex_path.exists() {
        return Err(AppError::NotFound(format!(
            "LaTeX not found for resume '{}'",
            id
        )));
    }

    let bytes = tokio::fs::read(&tex_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read LaTeX: {}", e)))?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-tex"),
    );
    if let Ok(val) = HeaderValue::from_str(&format!("attachment; filename=\"resume_{}.tex\"", id)) {
        headers.insert(header::CONTENT_DISPOSITION, val);
    }

    Ok((headers, bytes))
}

/// GET /api/resumes/:id/events
/// Non-WebSocket fallback endpoint returning all persisted generation events for a resume.
pub async fn get_resume_events(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<Vec<crate::services::event_bus::CanvasEvent>>, AppError> {
    let events = crate::services::event_bus::get_events_for_resume(&state.db, id)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to query generation events: {}", e))
        })?;

    Ok(Json(events))
}

pub async fn get_resume_evidence(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let path = state
        .config
        .data_dir
        .join("applications")
        .join(id.to_string())
        .join("evidence.json");
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::NotFound(format!("Evidence report for resume #{id} not found")))?;
    let report = serde_json::from_slice(&bytes).map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(report))
}
