use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: i64,
    pub repository_id: i64,
    pub name: String,
    pub summary: Option<String>,
    pub technologies_json: String,
    pub features_json: String,
    pub architecture_json: String,
    pub resume_points_json: String,
    pub confidence: f64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EvidenceRow {
    pub id: i64,
    pub project_id: i64,
    pub claim: String,
    pub source_file: String,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub commit_sha: Option<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectDetail {
    pub project: ProjectRow,
    pub evidence: Vec<EvidenceRow>,
}
