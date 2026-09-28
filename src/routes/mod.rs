pub mod github;
pub mod live;
pub mod projects;
pub mod resumes;
pub mod system;
pub mod templates;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::services::ServeDir;

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
        .with_state(state)
}

async fn health_check() -> &'static str {
    "OK"
}
