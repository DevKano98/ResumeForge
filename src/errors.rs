use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Internal(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
        };

        let body = Json(json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}
