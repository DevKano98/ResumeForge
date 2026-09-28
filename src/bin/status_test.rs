use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::routes::create_router;
use resumeforge::state::AppState;
use serde_json::Value;
use std::path::PathBuf;
use tower::util::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Checking System Status with Tectonic Installed & Cached ===");

    let db_path = PathBuf::from("data/resumeforge.db");
    let pool = db::init_db(&db_path).await?;
    let config = Config::default();
    let state = AppState::new(config, pool);

    let router = create_router(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    println!("{}", serde_json::to_string_pretty(&body)?);

    assert!(
        body["tectonic"]["installed"].as_bool().unwrap_or(false),
        "Tectonic should be detected as installed"
    );
    assert!(
        body["tectonic"]["bundle_cached"].as_bool().unwrap_or(false),
        "Bundle should be detected as cached"
    );
    println!("\n✓ Tectonic status is INSTALLED and BUNDLE CACHED!");

    Ok(())
}
