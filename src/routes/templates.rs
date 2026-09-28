use crate::db::templates as db_templates;
use crate::errors::AppError;
use crate::models::template::{
    CurrentTemplateResponse, MasterTemplate, SaveTemplateRequest, SaveTemplateResponse,
    StarterTemplateInfo,
};
use crate::state::AppState;
use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(serde::Deserialize)]
pub struct AdaptRequest {
    pub content: String,
}

pub async fn adapt_template(
    State(state): State<AppState>,
    Json(request): Json<AdaptRequest>,
) -> Result<Json<crate::services::template_adapt::AdaptPreview>, AppError> {
    let preview = crate::services::template_adapt::adapt(&request.content)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let result = crate::services::latex::compile_latex_content(
        &preview.adapted_content,
        &state.config.data_dir.join("temp"),
        None,
    )
    .await
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if !result.success {
        return Err(AppError::BadRequest(
            result
                .error_message
                .unwrap_or_else(|| "Adapted template did not compile".into()),
        ));
    }
    Ok(Json(preview))
}

const REQUIRED_PLACEHOLDERS: &[&str] = &[
    "\\ResumeSummary",
    "\\ResumeSkills",
    "\\ResumeExperience",
    "\\ResumeProjects",
    "\\tighten",
];

/// GET /api/template
/// Returns the current active master LaTeX template and metadata.
pub async fn get_master_template(
    State(state): State<AppState>,
) -> Result<Json<CurrentTemplateResponse>, AppError> {
    let master_path = state.config.data_dir.join("master").join("resume.tex");

    if !master_path.exists() {
        return Ok(Json(CurrentTemplateResponse {
            has_master: false,
            content: None,
            content_hash: None,
            file_path: None,
            is_starter_template: false,
            adapted_from_paste: false,
            created_at: None,
        }));
    }

    let content = tokio::fs::read_to_string(&master_path).await.map_err(|e| {
        AppError::Internal(anyhow::anyhow!("Failed to read master template: {}", e))
    })?;

    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let db_record = db_templates::get_current_master_template(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?;

    if let Some(record) = db_record {
        Ok(Json(CurrentTemplateResponse {
            has_master: true,
            content: Some(content),
            content_hash: Some(record.content_hash),
            file_path: Some(record.file_path),
            is_starter_template: record.is_starter_template != 0,
            adapted_from_paste: record.adapted_from_paste != 0,
            created_at: Some(record.created_at),
        }))
    } else {
        Ok(Json(CurrentTemplateResponse {
            has_master: true,
            content: Some(content),
            content_hash: Some(hash),
            file_path: Some(master_path.to_string_lossy().to_string()),
            is_starter_template: false,
            adapted_from_paste: false,
            created_at: None,
        }))
    }
}

/// POST /api/template
/// Validates, snapshots, saves master template, and records in database.
pub async fn save_master_template(
    State(state): State<AppState>,
    Json(payload): Json<SaveTemplateRequest>,
) -> Result<(StatusCode, Json<SaveTemplateResponse>), AppError> {
    let trimmed = payload.content.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "Template content cannot be empty".to_string(),
        ));
    }

    // Basic LaTeX document validation
    if !payload.content.contains("\\begin{document}")
        || !payload.content.contains("\\end{document}")
    {
        return Err(AppError::BadRequest(
            "Template must contain \\begin{document} and \\end{document}".to_string(),
        ));
    }

    // Placeholder macro validation
    let mut missing_macros = Vec::new();
    for placeholder in REQUIRED_PLACEHOLDERS {
        if !payload.content.contains(placeholder) {
            missing_macros.push(*placeholder);
        }
    }

    if !missing_macros.is_empty() {
        return Err(AppError::BadRequest(format!(
            "Template missing required placeholder macros: {}. Must contain \\ResumeSummary, \\ResumeSkills, \\ResumeExperience, \\ResumeProjects, and \\tighten",
            missing_macros.join(", ")
        )));
    }

    // Compile validation with Tectonic before saving in isolated temp dir
    let temp_compile_dir = state.config.data_dir.join("temp");
    let compile_res =
        crate::services::latex::compile_latex_content(&payload.content, &temp_compile_dir, None)
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Tectonic execution failure: {}", e))
            })?;

    if !compile_res.success {
        let line_info = compile_res
            .line_number
            .map(|l| format!(" at line {}", l))
            .unwrap_or_default();
        let err_msg = compile_res
            .error_message
            .unwrap_or_else(|| "LaTeX compilation failed".to_string());

        return Err(AppError::BadRequest(format!(
            "LaTeX compilation failed{}: {}",
            line_info, err_msg
        )));
    }

    let hash = format!("{:x}", Sha256::digest(payload.content.as_bytes()));
    let master_dir = state.config.data_dir.join("master");
    let history_dir = master_dir.join("master-history");

    tokio::fs::create_dir_all(&history_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create history dir: {}", e)))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let hash_short = if hash.len() >= 8 { &hash[..8] } else { &hash };
    let snapshot_filename = format!("resume_{}_{}.tex", timestamp, hash_short);
    let snapshot_path = history_dir.join(&snapshot_filename);

    tokio::fs::write(&snapshot_path, &payload.content)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to write history snapshot: {}", e))
        })?;

    let master_path = master_dir.join("resume.tex");
    tokio::fs::write(&master_path, &payload.content)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to write master template: {}", e))
        })?;

    let record = db_templates::save_master_template(
        &state.db,
        &hash,
        &master_path.to_string_lossy(),
        payload.is_starter_template,
        payload.adapted_from_paste,
    )
    .await
    .map_err(|e| {
        AppError::Internal(anyhow::anyhow!(
            "Failed to record template in database: {}",
            e
        ))
    })?;

    Ok((
        StatusCode::CREATED,
        Json(SaveTemplateResponse {
            id: record.id,
            content_hash: record.content_hash,
            file_path: record.file_path,
            snapshot_path: snapshot_path.to_string_lossy().to_string(),
            is_starter_template: record.is_starter_template != 0,
            adapted_from_paste: record.adapted_from_paste != 0,
            created_at: record.created_at,
        }),
    ))
}

/// GET /api/master/history
/// Lists historical master template versions.
pub async fn get_master_history(
    State(state): State<AppState>,
) -> Result<Json<Vec<MasterTemplate>>, AppError> {
    let history = db_templates::list_master_history(&state.db)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?;
    Ok(Json(history))
}

/// GET /api/master/history/:id
/// Fetches a specific historical template version and its content.
pub async fn get_master_history_by_id(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<i64>,
) -> Result<impl IntoResponse, AppError> {
    let template = db_templates::get_master_template_by_id(&state.db, id)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Database error: {}", e)))?
        .ok_or_else(|| AppError::NotFound(format!("Template version #{} not found", id)))?;

    // Try to find snapshot in history directory matching hash prefix
    let history_dir = state.config.data_dir.join("master").join("master-history");
    let mut content = None;

    if history_dir.exists() {
        if let Ok(mut entries) = tokio::fs::read_dir(&history_dir).await {
            let hash_short = if template.content_hash.len() >= 8 {
                &template.content_hash[..8]
            } else {
                &template.content_hash
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains(hash_short) && name.ends_with(".tex") {
                    if let Ok(c) = tokio::fs::read_to_string(entry.path()).await {
                        content = Some(c);
                        break;
                    }
                }
            }
        }
    }

    // Fallback: if this is the current active version, read from resume.tex
    if content.is_none() && template.superseded_at.is_none() {
        let master_path = Path::new(&template.file_path);
        if master_path.exists() {
            if let Ok(c) = tokio::fs::read_to_string(master_path).await {
                content = Some(c);
            }
        }
    }

    Ok(Json(json!({
        "template": template,
        "content": content,
    })))
}

/// GET /api/templates/starters
/// Lists available starter templates in data/templates/starter/
pub async fn get_starter_templates(
    State(state): State<AppState>,
) -> Result<Json<Vec<StarterTemplateInfo>>, AppError> {
    let starter_dir = state.config.data_dir.join("templates").join("starter");
    let mut templates = Vec::new();

    if starter_dir.exists() {
        let mut dir = tokio::fs::read_dir(&starter_dir).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to read starter dir: {}", e))
        })?;

        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("tex") {
                let filename = entry.file_name().to_string_lossy().to_string();
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("starter")
                    .to_string();
                let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();

                let (name, description) = match stem.as_str() {
                    "classic" => (
                        "Classic Single Column",
                        "Clean, traditional academic & professional format with standard serif/sans typography.",
                    ),
                    "modern" => (
                        "Modern Accent",
                        "Sleek layout with styled section dividers, subtle color accents, and compact spacing.",
                    ),
                    _ => (
                        "Custom Starter",
                        "Pre-configured template with ResumeForge placeholder macros.",
                    ),
                };

                templates.push(StarterTemplateInfo {
                    id: stem,
                    name: name.to_string(),
                    filename,
                    description: description.to_string(),
                    content,
                });
            }
        }
    }

    // Sort so classic comes first, then modern
    templates.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(templates))
}

/// GET /api/template/pdf
/// Compiles current master LaTeX template and serves rendered PDF.
pub async fn get_master_pdf(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let master_path = state.config.data_dir.join("master").join("resume.tex");
    if !master_path.exists() {
        return Err(AppError::NotFound(
            "Master resume template not found".to_string(),
        ));
    }

    let master_dir = state.config.data_dir.join("master");
    let compile_res = crate::services::latex::compile_latex_file(&master_path, &master_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("LaTeX compilation error: {}", e)))?;

    if !compile_res.success {
        let err_msg = compile_res
            .error_message
            .unwrap_or_else(|| "Compilation failed".to_string());
        return Err(AppError::BadRequest(format!(
            "Master template LaTeX error: {}",
            err_msg
        )));
    }

    let pdf_path = master_dir.join("resume.pdf");
    if !pdf_path.exists() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "PDF file was not produced"
        )));
    }

    let pdf_bytes = tokio::fs::read(&pdf_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read PDF: {}", e)))?;

    let headers = [
        (axum::http::header::CONTENT_TYPE, "application/pdf"),
        (
            axum::http::header::CONTENT_DISPOSITION,
            "inline; filename=\"master_resume.pdf\"",
        ),
    ];

    Ok((headers, pdf_bytes))
}

/// GET /api/template/facts
/// Returns parsed MasterFacts for the active master template.
pub async fn get_master_facts(
    State(state): State<AppState>,
) -> Result<Json<crate::models::generation::MasterFacts>, AppError> {
    let master_path = state.config.data_dir.join("master").join("resume.tex");

    if !master_path.exists() {
        return Ok(Json(crate::models::generation::MasterFacts::default()));
    }

    let content = tokio::fs::read_to_string(&master_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read master template: {}", e)))?;

    let facts = crate::services::master_parser::parse_master_facts(&content);
    Ok(Json(facts))
}
