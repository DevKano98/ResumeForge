use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::{config::Config, db, routes::create_router, state::AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::time::{sleep, Duration, Instant};
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(format!("data/test_step22_{}", uuid::Uuid::new_v4()));
    let master_dir = root.join("master");
    std::fs::create_dir_all(&master_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        master_dir.join("resume.tex"),
    )?;
    let pool = db::init_db(&root.join("test.db")).await?;
    let repo_id: i64 = sqlx::query_scalar("INSERT INTO repositories (github_repo_id, owner, name, url, visibility) VALUES ('fixture', 'test', 'resilient-queue', 'https://github.com/test/resilient-queue', 'public') RETURNING id")
        .fetch_one(&pool).await?;
    let project_id: i64 = sqlx::query_scalar("INSERT INTO projects (repository_id, name, summary, technologies_json, features_json, confidence) VALUES (?, 'ResilientQueue', 'Rust Tokio queue with SQLite WAL persistence', '[\"Rust\",\"Tokio\",\"SQLite\"]', '[\"durable queue\"]', 0.9) RETURNING id")
        .bind(repo_id).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO project_evidence (project_id, claim, source_file, line_start, line_end, commit_sha, confidence) VALUES (?, 'Implements a Rust Tokio queue with SQLite WAL persistence', 'README.md', 2, 2, 'abc123', 0.9)")
        .bind(project_id).execute(&pool).await?;
    let state = AppState::new(
        Config {
            host: "127.0.0.1".into(),
            port: 3000,
            data_dir: root.clone(),
            frontend_dir: PathBuf::from("frontend"),
        },
        pool.clone(),
    );
    let response = create_router(state.clone()).oneshot(Request::builder().method("POST").uri("/api/resumes")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({"company": "Example Corp", "role": "Senior Rust Engineer", "job_description": "Senior Rust Engineer. Build reliable Tokio services with SQLite WAL and distributed systems design."}).to_string()))?).await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    let id = created["id"].as_i64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(240);
    let final_row = loop {
        let row = db::resumes::get_resume_by_id(&pool, id).await?.unwrap();
        if [
            "ready",
            "ready_sparse",
            "github_error",
            "ai_empty_response",
            "ai_tool_denied",
            "ai_timeout",
            "ai_hung",
            "ai_invalid_json",
            "ai_error",
            "latex_error",
            "page_limit_error",
            "validation_error",
        ]
        .contains(&row.status.as_str())
        {
            break row;
        }
        assert!(
            Instant::now() < deadline,
            "Pipeline timed out in {}",
            row.status
        );
        sleep(Duration::from_millis(500)).await;
    };
    println!(
        "Pipeline status: {} {:?}",
        final_row.status, final_row.error_detail
    );
    let events = resumeforge::services::event_bus::get_events_for_resume(&pool, id).await?;
    println!("Canvas events: {}", events.len());
    assert!(
        ["ready", "ready_sparse"].contains(&final_row.status.as_str()),
        "Pipeline failed: {:?}",
        final_row.error_detail
    );
    assert_eq!(final_row.page_count, Some(1));
    let app_dir = root.join("applications").join(id.to_string());
    for file in [
        "input-context.json",
        "resume.json",
        "evidence.json",
        "resume.tex",
        "resume.pdf",
        "master-snapshot.tex",
    ] {
        assert!(app_dir.join(file).exists(), "Missing {file}");
    }

    let evidence_bytes = tokio::fs::read(app_dir.join("evidence.json")).await?;
    let evidence_json: serde_json::Value = serde_json::from_slice(&evidence_bytes)?;
    let accepted_count = evidence_json["accepted"].as_array().map_or(0, Vec::len);
    let rejected_count = evidence_json["rejected"].as_array().map_or(0, Vec::len);

    println!("\n=== STEP 22 CONCRETE NUMBERS ===");
    println!("status:               {}", final_row.status);
    println!("page_count:           {}", final_row.page_count.unwrap_or(0));
    println!("content_density_pct:  {:.1}%", final_row.content_density_pct.unwrap_or(0.0));
    println!("accepted_claims:      {}", accepted_count);
    println!("rejected_claims:      {}", rejected_count);
    println!("evidence.json:        {}", serde_json::to_string(&evidence_json)?);
    assert!(events
        .iter()
        .any(|event| event.node == "evidence_validation" && event.event_type == "node_done"));
    assert!(events
        .iter()
        .any(|event| event.node == "content_generation" && event.event_type == "command_output"));
    drop(state);
    pool.close().await;
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&root) {
            Ok(()) => break,
            Err(_) if attempt < 19 => sleep(Duration::from_millis(100)).await,
            Err(error) => return Err(error.into()),
        }
    }
    println!("Step 22 passed: evidence-grounded one-page resume and persisted Canvas events");
    Ok(())
}
