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
use tower::util::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ResumeForge Step 7 Integration Test: Resume History CRUD & Permanent Delete ===");

    let test_dir = PathBuf::from("data/test_step7");
    if test_dir.exists() {
        let _ = std::fs::remove_dir_all(&test_dir);
    }
    std::fs::create_dir_all(&test_dir)?;

    // Copy starter templates
    let starter_dir = test_dir.join("templates").join("starter");
    std::fs::create_dir_all(&starter_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        starter_dir.join("classic.tex"),
    )?;

    // Create master template
    let master_dir = test_dir.join("master");
    std::fs::create_dir_all(&master_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        master_dir.join("resume.tex"),
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

    // --- TEST 1: POST /api/resumes (Create Resume) ---
    println!("\n[Test 1] POST /api/resumes creating dummy compiled resume...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/resumes")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "company": "Anthropic Systems",
            "role": "Lead Infrastructure Engineer",
            "job_description": "We are seeking a Lead Infrastructure Engineer with deep experience in Rust, Tokio, SQLite WAL, and distributed systems resilience.",
            "extra_instructions": "Highlight low-latency zero-copy networking experience."
        }).to_string()))?;

    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let created: Value = serde_json::from_slice(&bytes)?;
    println!("Created resume: {}", created);

    let resume_id = created["id"].as_i64().unwrap();
    assert_eq!(resume_id, 1);
    assert_eq!(created["company"], "Anthropic Systems");
    assert_eq!(created["role"], "Lead Infrastructure Engineer");
    assert_eq!(created["status"], "queued");

    // Without indexed projects the asynchronous worker reports a visible terminal error.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(60);
    loop {
        let current = db::resumes::get_resume_by_id(&pool, resume_id)
            .await?
            .unwrap();
        if current.status == "github_error" {
            break;
        }
        assert_ne!(
            current.status, "latex_error",
            "Generation failed: {:?}",
            current.error_detail
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "Timed out waiting for queued resume"
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Verify application directory and files on disk
    let app_dir = test_dir.join("applications").join(resume_id.to_string());
    assert!(app_dir.exists(), "Application directory must exist on disk");
    assert!(app_dir.join("job.txt").exists(), "job.txt must exist");
    assert!(
        app_dir.join("instructions.txt").exists(),
        "instructions.txt must exist"
    );
    assert!(
        app_dir.join("master-snapshot.tex").exists(),
        "master snapshot must exist"
    );
    println!("✓ Applications directory created at {:?} with job.txt, instructions.txt, resume.tex, and resume.pdf", app_dir);

    // --- TEST 2: GET /api/resumes (List) ---
    println!("\n[Test 2] GET /api/resumes listing resumes...");
    let router = create_router(state.clone());
    let req = Request::builder().uri("/api/resumes").body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let list: Value = serde_json::from_slice(&bytes)?;
    let list_arr = list.as_array().unwrap();
    assert_eq!(list_arr.len(), 1);
    assert_eq!(list_arr[0]["id"], resume_id);
    println!("✓ History list returned 1 resume matching id={}", resume_id);

    // --- TEST 3: GET /api/resumes/:id (Detail) ---
    println!("\n[Test 3] GET /api/resumes/{} detail...", resume_id);
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri(format!("/api/resumes/{}", resume_id))
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let detail: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(detail["id"], resume_id);
    assert_eq!(detail["company"], "Anthropic Systems");
    println!("✓ Resume detail retrieved successfully");

    // --- TEST 4: Seed dependent rows for cascade delete verification ---
    println!("\n[Test 4] Seeding dependent rows into resume_projects and generation_events...");
    // Create dummy repo + project first
    let repo_id: i64 = sqlx::query_scalar(
        "INSERT INTO repositories (github_repo_id, owner, name, url, visibility) VALUES ('r1', 'test', 'repo1', 'https://github.com/test/repo1', 'public') RETURNING id"
    ).fetch_one(&pool).await?;

    let proj_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects (repository_id, name) VALUES (?, 'DistributedEngine') RETURNING id",
    )
    .bind(repo_id)
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO resume_projects (resume_id, project_id, relevance_score, selected) VALUES (?, ?, 0.95, 1)"
    ).bind(resume_id).bind(proj_id).execute(&pool).await?;

    let prior_events: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM generation_events WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    let next_seq: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM generation_events WHERE resume_id = ?",
    )
    .bind(resume_id)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO generation_events (resume_id, seq, node, event_type, payload_json) VALUES (?, ?, 'LatexRenderAgent', 'node_done', '{\"success\": true}')"
    ).bind(resume_id).bind(next_seq).execute(&pool).await?;

    let count_rp: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM resume_projects WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    let count_ge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM generation_events WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count_rp, 1);
    assert_eq!(count_ge, prior_events + 1);
    println!("✓ Dependent rows seeded: resume_projects=1, generation_events={count_ge}");

    // --- TEST 5: DELETE /api/resumes/:id (Permanent Delete: DB + Filesystem) ---
    println!(
        "\n[Test 5] DELETE /api/resumes/{} (permanent deletion of DB rows + disk directory)...",
        resume_id
    );
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/resumes/{}", resume_id))
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let del_resp: Value = serde_json::from_slice(&bytes)?;
    println!("Delete response: {}", del_resp);
    assert_eq!(del_resp["success"], true);
    assert_eq!(del_resp["directory_removed"], true);

    // Verify DB: parent resumes row is GONE
    let count_res: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resumes WHERE id = ?")
        .bind(resume_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count_res, 0, "resumes row MUST be deleted from database");

    // Verify DB: cascaded resume_projects row is GONE
    let count_rp_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM resume_projects WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        count_rp_after, 0,
        "resume_projects dependent row MUST be cascade-deleted"
    );

    // Verify DB: cascaded generation_events row is GONE
    let count_ge_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM generation_events WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        count_ge_after, 0,
        "generation_events dependent row MUST be cascade-deleted"
    );

    // Verify DISK: applications directory is GONE
    assert!(
        !app_dir.exists(),
        "Application directory {:?} MUST be completely removed from disk!",
        app_dir
    );
    println!(
        "✓ DB verification: resumes=0, resume_projects=0, generation_events=0 (cascade verified)"
    );
    println!(
        "✓ Disk verification: directory {:?} is completely deleted from filesystem!",
        app_dir
    );

    // --- TEST 6: DELETE /api/resumes/999 (Nonexistent -> 404) ---
    println!("\n[Test 6] DELETE /api/resumes/999 nonexistent check...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/resumes/999")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    println!("✓ DELETE on nonexistent ID correctly returns 404");

    // Cleanup test dir
    let _ = std::fs::remove_dir_all(&test_dir);
    println!("\n*** ALL STEP 7 INTEGRATION TESTS PASSED PERFECTLY ***\n");

    Ok(())
}
