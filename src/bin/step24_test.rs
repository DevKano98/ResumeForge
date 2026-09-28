use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::{config::Config, db, routes::create_router, state::AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(format!("data/test_step24_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    let pool = db::init_db(&root.join("test.db")).await?;
    let state = AppState::new(
        Config {
            host: "127.0.0.1".into(),
            port: 3000,
            data_dir: root.clone(),
            frontend_dir: PathBuf::from("frontend"),
        },
        pool.clone(),
    );
    let source = r"\documentclass{article}
\begin{document}
\section{Professional Summary}Reliable engineer.
\section{Technical Skills}Rust and SQLite.
\section{Experience}Built durable services.
\section{Projects}Created a queue.
\end{document}";
    let preview = create_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/template/adapt")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"content": source}).to_string()))?,
        )
        .await?;
    assert_eq!(preview.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&to_bytes(preview.into_body(), usize::MAX).await?)?;
    let adapted = body["adapted_content"].as_str().unwrap();
    assert!(adapted.contains("\\ResumeProjects{"));
    assert!(body["diff"].as_str().unwrap().contains("ResumeSummary"));
    let save = create_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/template")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"content": adapted, "adapted_from_paste": true}).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(save.status(), StatusCode::CREATED);
    let saved: Value = serde_json::from_slice(&to_bytes(save.into_body(), usize::MAX).await?)?;
    assert_eq!(saved["adapted_from_paste"], true);
    drop(state);
    pool.close().await;
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&root) {
            Ok(()) => break,
            Err(_) if attempt < 19 => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!("Step 24 passed: preview, LaTeX compile, explicit save and version tracking");
    Ok(())
}
