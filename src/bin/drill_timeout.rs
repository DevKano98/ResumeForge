use resumeforge::{config::Config, db, services::{event_bus::EventBus, resume_generator}};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Override content generation timeout to 5 seconds
    std::env::set_var("RESUMEFORGE_CONTENT_GEN_TIMEOUT_SECS", "5");

    let test_dir = PathBuf::from(format!("data/test_drill_timeout_{}", uuid::Uuid::new_v4()));
    let master_dir = test_dir.join("master");
    std::fs::create_dir_all(&master_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        master_dir.join("resume.tex"),
    )?;

    let db_path = test_dir.join("test.db");
    let pool = db::init_db(&db_path).await?;
    let bus = EventBus::new();
    let config = Config {
        host: "127.0.0.1".into(),
        port: 3000,
        data_dir: test_dir.clone(),
        frontend_dir: PathBuf::from("frontend"),
    };

    // Fixture repository & project
    let repo_id: i64 = sqlx::query_scalar(
        "INSERT INTO repositories (github_repo_id, owner, name, url, visibility) VALUES ('fixture_to', 'test', 'timeout-repo', 'https://github.com/test/timeout-repo', 'public') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects (repository_id, name, summary, technologies_json, features_json, confidence) VALUES (?, 'TimeoutProject', 'Async queue', '[\"Rust\"]', '[\"queue\"]', 0.9) RETURNING id",
    )
    .bind(repo_id)
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO project_evidence (project_id, claim, source_file, line_start, line_end, commit_sha, confidence) VALUES (?, 'Async queue with WAL', 'README.md', 1, 2, 'sha1', 0.9)",
    )
    .bind(project_id)
    .execute(&pool)
    .await?;

    let resume = db::resumes::insert_resume(
        &pool,
        Some("ExampleCorp"),
        Some("Rust Engineer"),
        "Senior Rust Engineer to build async Tokio services.",
        None,
        None,
        "queued",
    )
    .await?;

    println!("Starting real generation drill with 5s timeout on resume #{}...", resume.id);
    let result = resume_generator::run(resume.id, &config, &pool, &bus).await;
    println!("Pipeline run completed with result: {:?}", result.as_ref().map_err(|e| (&e.status, &e.stage, &e.detail)));

    // If run returned Err, update resume status just as generation_queue does
    if let Err(err) = result {
        sqlx::query(
            "UPDATE resumes SET status = ?, error_stage = ?, error_detail = ?, completed_at = datetime('now') WHERE id = ?"
        )
        .bind(err.status)
        .bind(err.stage)
        .bind(&err.detail)
        .bind(resume.id)
        .execute(&pool)
        .await?;

        let _ = resumeforge::services::event_bus::emit_event(
            &pool,
            &bus,
            resume.id,
            err.stage,
            "node_error",
            serde_json::json!({"stage": err.status, "detail": err.detail}),
        )
        .await;
    }

    let final_resume = db::resumes::get_resume_by_id(&pool, resume.id).await?.unwrap();
    println!("\n=== FINAL RESUME STATUS IN DATABASE ===");
    println!("id: {}", final_resume.id);
    println!("status: {}", final_resume.status);
    println!("error_stage: {:?}", final_resume.error_stage);
    println!("error_detail: {:?}", final_resume.error_detail);

    println!("\n=== GENERATION_EVENTS ROWS IN DATABASE ===");
    let rows = resumeforge::services::event_bus::get_events_for_resume(&pool, resume.id).await?;

    for row in &rows {
        println!("seq {:2} | node: {:18} | event_type: {:15} | payload: {}", row.seq, row.node, row.event_type, row.payload);
    }

    pool.close().await;
    let _ = std::fs::remove_dir_all(&test_dir);
    Ok(())
}
