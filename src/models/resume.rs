use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResumeRow {
    pub id: i64,
    pub company: Option<String>,
    pub role: Option<String>,
    pub job_description: String,
    pub extra_instructions: Option<String>,
    pub status: String,
    pub error_stage: Option<String>,
    pub error_detail: Option<String>,
    pub parent_resume_id: Option<i64>,
    pub pdf_path: Option<String>,
    pub tex_path: Option<String>,
    pub master_sha256: Option<String>,
    pub page_count: Option<i64>,
    pub content_density_pct: Option<f64>,
    pub compact_mode_applied: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResumeRequest {
    pub company: Option<String>,
    pub role: Option<String>,
    pub job_description: String,
    pub extra_instructions: Option<String>,
    pub parent_resume_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResumeResponse {
    pub success: bool,
    pub deleted_id: i64,
    pub directory_removed: bool,
}
