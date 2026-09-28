use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::routes::create_router;
use resumeforge::state::AppState;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tower::util::ServiceExt;

struct TestDir(PathBuf);
impl TestDir {
    fn new(prefix: &str) -> Self {
        let p = std::env::temp_dir().join(format!("rf_test_{}_{}", prefix, uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&p);
        Self(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn test_template_validation_and_history() -> anyhow::Result<()> {
    let tmp = TestDir::new("templates");
    let test_dir = tmp.path().to_path_buf();

    // Copy starter templates into test_dir/templates/starter
    let starter_dir = test_dir.join("templates").join("starter");
    std::fs::create_dir_all(&starter_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        starter_dir.join("classic.tex"),
    )?;
    std::fs::copy(
        "data/templates/starter/modern.tex",
        starter_dir.join("modern.tex"),
    )?;

    let db_path = test_dir.join("test.db");
    let pool = db::init_db(&db_path).await?;

    let config = Config {
        host: "127.0.0.1".to_string(),
        port: 3000,
        data_dir: test_dir.clone(),
        frontend_dir: PathBuf::from("frontend"),
    };

    let state = AppState::new(config, pool.clone());

    // 1. GET /api/templates/starters
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/templates/starters")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let starters: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(starters.as_array().unwrap().len(), 2);

    // 2. GET /api/template before any save -> has_master: false
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["has_master"], false);

    // 3. Validation rejection: empty content (400)
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": "   " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 4. Validation rejection: missing required macro (422)
    let invalid_macro_tex = r#"\documentclass{article}
\begin{document}
\newcommand{\tighten}{}
Missing ResumeSummary macro!
\end{document}"#;
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": invalid_macro_tex }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 5. Save valid starter template (200 OK)
    let classic_src = std::fs::read_to_string("data/templates/starter/classic.tex")?;
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": classic_src, "is_starter_template": true }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 6. Verify has_master: true after save
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["has_master"], true);
    assert!(!body["content_hash"].as_str().unwrap().is_empty());

    // 7. Verify template history endpoint
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/master/history")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let history: Value = serde_json::from_slice(&bytes)?;
    assert!(!history.as_array().unwrap().is_empty());

    Ok(())
}
