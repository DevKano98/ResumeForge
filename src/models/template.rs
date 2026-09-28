use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MasterTemplate {
    pub id: i64,
    pub content_hash: String,
    pub file_path: String,
    pub is_starter_template: i64,
    pub adapted_from_paste: i64,
    pub created_at: String,
    pub superseded_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarterTemplateInfo {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTemplateResponse {
    pub has_master: bool,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub file_path: Option<String>,
    pub is_starter_template: bool,
    pub adapted_from_paste: bool,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTemplateRequest {
    pub content: String,
    #[serde(default)]
    pub is_starter_template: bool,
    #[serde(default)]
    pub adapted_from_paste: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveTemplateResponse {
    pub id: i64,
    pub content_hash: String,
    pub file_path: String,
    pub snapshot_path: String,
    pub is_starter_template: bool,
    pub adapted_from_paste: bool,
    pub created_at: String,
}
