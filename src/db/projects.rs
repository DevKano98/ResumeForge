use crate::models::project::{EvidenceRow, ProjectDetail, ProjectRow};
use sqlx::SqlitePool;

const PROJECT_COLUMNS: &str = "id, repository_id, name, summary, technologies_json, features_json, architecture_json, resume_points_json, confidence, updated_at";
const EVIDENCE_COLUMNS: &str =
    "id, project_id, claim, source_file, line_start, line_end, commit_sha, confidence";

pub async fn list(pool: &SqlitePool) -> Result<Vec<ProjectRow>, sqlx::Error> {
    sqlx::query_as::<_, ProjectRow>(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY confidence DESC, id DESC"
    ))
    .fetch_all(pool)
    .await
}

pub async fn get_detail(pool: &SqlitePool, id: i64) -> Result<Option<ProjectDetail>, sqlx::Error> {
    let project = sqlx::query_as::<_, ProjectRow>(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects WHERE id=?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(project) = project else {
        return Ok(None);
    };
    let evidence = sqlx::query_as::<_, EvidenceRow>(&format!(
        "SELECT {EVIDENCE_COLUMNS} FROM project_evidence WHERE project_id=? ORDER BY id"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Some(ProjectDetail { project, evidence }))
}

pub async fn evidence_for_project(
    pool: &SqlitePool,
    project_id: i64,
) -> Result<Vec<EvidenceRow>, sqlx::Error> {
    sqlx::query_as::<_, EvidenceRow>(&format!(
        "SELECT {EVIDENCE_COLUMNS} FROM project_evidence WHERE project_id=? ORDER BY id"
    ))
    .bind(project_id)
    .fetch_all(pool)
    .await
}
