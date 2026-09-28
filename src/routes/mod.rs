pub mod github;
pub mod live;
pub mod projects;
pub mod resumes;
pub mod system;
pub mod templates;

use crate::state::AppState;
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, Method, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use tower_http::services::ServeDir;

/// Rejects requests with invalid Host (DNS rebinding) or missing/forged Origin on mutating/WS requests.
pub async fn security_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let port = state.config.port;

    // 1. Host header validation (DNS rebinding protection)
    let host_header = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase());

    let is_valid_host = match host_header {
        Some(ref host) => {
            let allowed_localhost = format!("localhost:{}", port);
            let allowed_127 = format!("127.0.0.1:{}", port);
            host == &allowed_localhost
                || host == &allowed_127
                || (port == 80 && (host == "localhost" || host == "127.0.0.1"))
        }
        None => false,
    };

    if !is_valid_host {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "Forbidden: invalid or missing Host header",
                "detail": "Requests must target localhost or 127.0.0.1 with the configured server port"
            })),
        )
            .into_response();
    }

    // 2. Origin check for POST, PUT, DELETE, and WebSocket upgrades (CSRF & CSWSH protection)
    let is_ws_upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
        || req.uri().path().starts_with("/ws/");

    let is_state_changing = matches!(
        req.method(),
        &Method::POST | &Method::PUT | &Method::DELETE
    );

    if is_state_changing || is_ws_upgrade {
        let origin_header = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_ascii_lowercase());

        let is_valid_origin = match origin_header {
            Some(ref origin) => {
                let allowed_localhost = format!("http://localhost:{}", port);
                let allowed_127 = format!("http://127.0.0.1:{}", port);
                origin == &allowed_localhost
                    || origin == &allowed_127
                    || (port == 80 && (origin == "http://localhost" || origin == "http://127.0.0.1"))
            }
            None => false,
        };

        if !is_valid_origin {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Forbidden: invalid or missing Origin header",
                    "detail": "Cross-origin requests and unauthenticated WebSocket upgrades are forbidden"
                })),
            )
                .into_response();
        }
    }

    next.run(req).await
}

pub fn create_router(state: AppState) -> Router {
    let frontend_dir = state.config.frontend_dir.clone();

    // Serve static files from frontend/ directory
    let serve_dir = ServeDir::new(&frontend_dir).append_index_html_on_directories(true);

    Router::new()
        .route("/api/health", get(health_check))
        .route("/api/github/status", get(github::get_status))
        .route("/api/github/sync", axum::routing::post(github::sync))
        .route("/api/projects", get(projects::list))
        .route("/api/projects/{id}", get(projects::detail))
        .route("/api/system/status", get(system::get_system_status))
        .route(
            "/api/system/antigravity-probe",
            get(system::get_antigravity_probe),
        )
        .route(
            "/api/template",
            get(templates::get_master_template).post(templates::save_master_template),
        )
        .route("/api/template/facts", get(templates::get_master_facts))
        .route("/api/template/pdf", get(templates::get_master_pdf))
        .route(
            "/api/template/adapt",
            axum::routing::post(templates::adapt_template),
        )
        .route("/api/master/history", get(templates::get_master_history))
        .route(
            "/api/master/history/{id}",
            get(templates::get_master_history_by_id),
        )
        .route(
            "/api/templates/starters",
            get(templates::get_starter_templates),
        )
        .route(
            "/api/resumes",
            get(resumes::list_resumes).post(resumes::create_resume),
        )
        .route(
            "/api/resumes/{id}",
            get(resumes::get_resume).delete(resumes::delete_resume),
        )
        .route(
            "/api/resumes/{id}/regenerate",
            axum::routing::post(resumes::regenerate_resume),
        )
        .route("/api/resumes/{id}/events", get(resumes::get_resume_events))
        .route(
            "/api/resumes/{id}/evidence",
            get(resumes::get_resume_evidence),
        )
        .route("/api/resumes/{id}/pdf", get(resumes::get_resume_pdf))
        .route("/api/resumes/{id}/tex", get(resumes::get_resume_tex))
        .route("/ws/resumes/{id}/live", get(live::ws_resume_live))
        .fallback_service(serve_dir)
        .layer(from_fn_with_state(state.clone(), security_middleware))
        .layer(DefaultBodyLimit::max(512 * 1024))
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK"
}
