use crate::models::resume::ResumeRow;
use sqlx::SqlitePool;

pub async fn insert_resume(
    pool: &SqlitePool,
    company: Option<&str>,
    role: Option<&str>,
    job_description: &str,
    extra_instructions: Option<&str>,
    parent_resume_id: Option<i64>,
    status: &str,
) -> Result<ResumeRow, sqlx::Error> {
    sqlx::query_as::<_, ResumeRow>(
        "INSERT INTO resumes (company, role, job_description, extra_instructions, parent_resume_id, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, datetime('now')) \
         RETURNING id, company, role, job_description, extra_instructions, status, error_stage, error_detail, \
                   parent_resume_id, pdf_path, tex_path, master_sha256, page_count, content_density_pct, \
                   compact_mode_applied, created_at, completed_at"
    )
    .bind(company)
    .bind(role)
    .bind(job_description)
    .bind(extra_instructions)
    .bind(parent_resume_id)
    .bind(status)
    .fetch_one(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn update_resume_artifacts(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    pdf_path: &str,
    tex_path: &str,
    master_sha256: &str,
    page_count: i64,
    content_density_pct: f64,
) -> Result<ResumeRow, sqlx::Error> {
    sqlx::query_as::<_, ResumeRow>(
        "UPDATE resumes SET \
            status = ?, \
            pdf_path = ?, \
            tex_path = ?, \
            master_sha256 = ?, \
            page_count = ?, \
            content_density_pct = ?, \
            completed_at = datetime('now') \
         WHERE id = ? \
         RETURNING id, company, role, job_description, extra_instructions, status, error_stage, error_detail, \
                   parent_resume_id, pdf_path, tex_path, master_sha256, page_count, content_density_pct, \
                   compact_mode_applied, created_at, completed_at"
    )
    .bind(status)
    .bind(pdf_path)
    .bind(tex_path)
    .bind(master_sha256)
    .bind(page_count)
    .bind(content_density_pct)
    .bind(id)
    .fetch_one(pool)
    .await
}

pub async fn list_resumes(pool: &SqlitePool) -> Result<Vec<ResumeRow>, sqlx::Error> {
    sqlx::query_as::<_, ResumeRow>(
        "SELECT id, company, role, job_description, extra_instructions, status, error_stage, error_detail, \
                parent_resume_id, pdf_path, tex_path, master_sha256, page_count, content_density_pct, \
                compact_mode_applied, created_at, completed_at \
         FROM resumes \
         ORDER BY id DESC"
    )
    .fetch_all(pool)
    .await
}

pub async fn get_resume_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<ResumeRow>, sqlx::Error> {
    sqlx::query_as::<_, ResumeRow>(
        "SELECT id, company, role, job_description, extra_instructions, status, error_stage, error_detail, \
                parent_resume_id, pdf_path, tex_path, master_sha256, page_count, content_density_pct, \
                compact_mode_applied, created_at, completed_at \
         FROM resumes \
         WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_resume_by_id(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM resumes WHERE id = ? AND status IN (\
         'queued','ready','ready_sparse','github_error','ai_empty_response',\
         'ai_tool_denied','ai_timeout','ai_hung','ai_invalid_json','ai_error',\
         'latex_error','page_limit_error','validation_error','cancelled')",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
