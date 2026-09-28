use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::routes::create_router;
use resumeforge::state::AppState;
use serde_json::{json, Value};
use std::path::PathBuf;
use tower::ServiceExt;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let unique = format!("{}_{}", prefix, uuid::Uuid::new_v4());
        let path = std::env::temp_dir().join("resumeforge_sec_tests").join(unique);
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

async fn setup_test_app() -> (AppState, TestDir) {
    let test_dir = TestDir::new("security");
    let config = Config {
        host: "127.0.0.1".to_string(),
        data_dir: test_dir.path.join("data"),
        frontend_dir: PathBuf::from("frontend"),
        port: 3000,
    };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let db_path = config.data_dir.join("test.db");
    let pool = db::init_db(&db_path).await.unwrap();
    let state = AppState::new(config, pool);
    (state, test_dir)
}

#[tokio::test]
async fn test_host_header_validation() -> anyhow::Result<()> {
    let (state, _td) = setup_test_app().await;

    // 1. Valid Host: localhost:3000 -> 200 OK
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .header("Host", "localhost:3000")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. Valid Host: 127.0.0.1:3000 -> 200 OK
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .header("Host", "127.0.0.1:3000")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Forged Host: evil.com (DNS rebinding) -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .header("Host", "evil.com")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(body["error"].as_str().unwrap().contains("invalid or missing Host"));

    // 4. Forged Host: attacker.com:3000 -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .header("Host", "attacker.com:3000")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 5. Forged Host: 192.168.1.5:3000 (LAN access attempt) -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .header("Host", "192.168.1.5:3000")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 6. Missing Host header -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/health")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    Ok(())
}

#[tokio::test]
async fn test_origin_header_validation_and_websocket_protection() -> anyhow::Result<()> {
    let (state, _td) = setup_test_app().await;

    // 1. Valid Origin on POST -> succeeds (reaches handler)
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": " " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    // Reached template handler which returns BAD_REQUEST on empty content
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 2. Forged Origin on POST: http://evil.com -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Origin", "http://evil.com")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": " " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(body["error"].as_str().unwrap().contains("invalid or missing Origin"));

    // 3. Forged Origin on POST: http://localhost:8080 (port mismatch) -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:8080")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": " " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 4. Missing Origin on POST -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": " " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 5. Forged Origin on DELETE -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/resumes/999")
        .header("Host", "localhost:3000")
        .header("Origin", "http://attacker.com")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 6. WebSocket upgrade with forged Origin -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/ws/resumes/1/live")
        .header("Host", "localhost:3000")
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Origin", "http://evil.com")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // 7. WebSocket upgrade with missing Origin -> 403 Forbidden
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/ws/resumes/1/live")
        .header("Host", "localhost:3000")
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    Ok(())
}

#[tokio::test]
async fn test_body_size_limits() -> anyhow::Result<()> {
    let (state, _td) = setup_test_app().await;

    // 1. Job description > 64 KB -> 400 Bad Request with clear message
    let oversized_jd = "A".repeat(65 * 1024); // 65 KB
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/resumes")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "job_description": oversized_jd
            })
            .to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(body["error"].as_str().unwrap().contains("Job description exceeds maximum allowed size of 64 KB"));

    // 2. Extra instructions > 16 KB -> 400 Bad Request with clear message
    let oversized_extra = "B".repeat(17 * 1024); // 17 KB
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/resumes")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "job_description": "Valid job description",
                "extra_instructions": oversized_extra
            })
            .to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(body["error"].as_str().unwrap().contains("Extra instructions exceed maximum allowed size of 16 KB"));

    // 3. Template upload > 512 KB -> 400 Bad Request with clear message
    let oversized_template = "C".repeat(520 * 1024); // 520 KB
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "content": oversized_template
            })
            .to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    // Either 400 with our message or 413 from body limit layer
    assert!(
        resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::PAYLOAD_TOO_LARGE,
        "Expected 400 or 413, got {}",
        resp.status()
    );

    // 4. Raw HTTP body exceeding DefaultBodyLimit (512 KB) -> 413 Payload Too Large
    let huge_body = vec![b'X'; 600 * 1024]; // 600 KB
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Host", "localhost:3000")
        .header("Origin", "http://localhost:3000")
        .header("Content-Type", "application/json")
        .body(Body::from(huge_body))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);

    Ok(())
}

#[tokio::test]
async fn test_tokens_never_exposed_in_responses() -> anyhow::Result<()> {
    let (state, _td) = setup_test_app().await;

    // Check /api/github/status response payload
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/github/status")
        .header("Host", "localhost:3000")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let raw_text = String::from_utf8_lossy(&bytes).to_lowercase();
    assert!(!raw_text.contains("token"));
    assert!(!raw_text.contains("secret"));
    assert!(!raw_text.contains("ghp_"));
    assert!(!raw_text.contains("gho_"));

    Ok(())
}
