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
    println!("=== ResumeForge Step 5 Integration Test: Templates & Snapshots ===");

    // Use a clean test data directory
    let test_dir = PathBuf::from("data/test_templates");
    if test_dir.exists() {
        let _ = std::fs::remove_dir_all(&test_dir);
    }
    std::fs::create_dir_all(&test_dir)?;

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

    // --- TEST 1: GET /api/templates/starters ---
    println!("\n[Test 1] GET /api/templates/starters...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/templates/starters")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let starters: Value = serde_json::from_slice(&bytes)?;
    println!(
        "Found {} starter templates",
        starters.as_array().unwrap().len()
    );
    assert_eq!(starters.as_array().unwrap().len(), 2);

    let classic_content = starters[0]["content"].as_str().unwrap().to_string();
    let modern_content = starters[1]["content"].as_str().unwrap().to_string();

    assert!(classic_content.contains("\\ResumeSummary"));
    assert!(classic_content.contains("\\tighten"));
    assert!(modern_content.contains("\\ResumeSummary"));
    assert!(modern_content.contains("\\tighten"));
    println!("✓ Starters valid and contain required macros: classic, modern");

    // --- TEST 2: GET /api/template before any save ---
    println!("\n[Test 2] GET /api/template before save...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["has_master"], false);
    println!("✓ Correctly reports has_master: false");

    // --- TEST 3: POST /api/template validation rejections ---
    println!("\n[Test 3] POST /api/template validation checks...");
    // 3a: Empty content
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": "   " }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    println!("✓ Empty content rejected (400)");

    // 3b: Missing \begin{document}
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "content": "Just raw text without LaTeX tags" }).to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    println!("✓ Missing LaTeX document tags rejected (400)");

    // 3c: Missing required macros (e.g. \ResumeProjects missing)
    let router = create_router(state.clone());
    let incomplete_latex = "\\documentclass{article}\\begin{document}\\ResumeSummary{A}\\ResumeSkills{B}\\ResumeExperience{C}\\tighten\\end{document}";
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "content": incomplete_latex }).to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    println!("Rejection response: {}", body);
    assert!(body["error"].as_str().unwrap().contains("\\ResumeProjects"));
    println!("✓ Missing placeholder macro rejected with explicit error message (400)");

    // 3d: Broken LaTeX that passes macro check but fails Tectonic compilation
    let router = create_router(state.clone());
    let broken_latex = r#"\documentclass{article}
\newcommand{\ResumeSummary}[1]{#1}
\newcommand{\ResumeSkills}[1]{#1}
\newcommand{\ResumeExperience}[1]{#1}
\newcommand{\ResumeProjects}[1]{#1}
\newcommand{\tighten}{}
\begin{document}
\ResumeSummary{Valid Summary}
\ResumeSkills{Valid Skills}
\ResumeExperience{Valid Exp}
\ResumeProjects{Valid Proj}
\undefinedCommandCausingTectonicError{fail}
\end{document}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "content": broken_latex }).to_string()))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    let err_str = body["error"].as_str().unwrap();
    println!("Deliberate compile failure response: {}", body);
    assert!(
        err_str.contains("LaTeX compilation failed"),
        "Must report LaTeX compilation failed"
    );
    // Verify file and DB were untouched
    assert!(
        !test_dir.join("master").join("resume.tex").exists(),
        "resume.tex must NOT be written when compile fails"
    );
    println!("✓ Broken LaTeX rejected with Tectonic error and master file not saved (400)");

    // --- TEST 4: POST /api/template valid save (Classic) ---
    println!("\n[Test 4] POST /api/template save Classic starter...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "content": classic_content,
                "is_starter_template": true,
                "adapted_from_paste": false
            })
            .to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let save1_resp: Value = serde_json::from_slice(&bytes)?;
    println!("Saved template version 1: {}", save1_resp);
    assert_eq!(save1_resp["id"], 1);
    assert_eq!(save1_resp["is_starter_template"], true);

    // Verify master file on disk
    let master_file = test_dir.join("master").join("resume.tex");
    assert!(master_file.exists());
    let read_master = std::fs::read_to_string(&master_file)?;
    assert_eq!(read_master, classic_content);
    println!("✓ data/master/resume.tex written and matches content");

    // Verify snapshot on disk
    let snapshot_path = PathBuf::from(save1_resp["snapshot_path"].as_str().unwrap());
    assert!(snapshot_path.exists());
    let read_snapshot = std::fs::read_to_string(&snapshot_path)?;
    assert_eq!(read_snapshot, classic_content);
    println!(
        "✓ data/master/master-history snapshot written: {:?}",
        snapshot_path.file_name().unwrap()
    );

    // --- TEST 5: GET /api/template after save ---
    println!("\n[Test 5] GET /api/template after save...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let get_resp: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(get_resp["has_master"], true);
    assert_eq!(get_resp["content_hash"], save1_resp["content_hash"]);
    assert_eq!(get_resp["is_starter_template"], true);
    println!("✓ GET /api/template correctly returns active master metadata & hash");

    // --- TEST 6: POST /api/template update (Modern) and verify superseding ---
    println!("\n[Test 6] POST /api/template update to Modern starter...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .method("POST")
        .uri("/api/template")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "content": modern_content,
                "is_starter_template": true,
                "adapted_from_paste": false
            })
            .to_string(),
        ))?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let save2_resp: Value = serde_json::from_slice(&bytes)?;
    println!("Saved template version 2: {}", save2_resp);
    assert_eq!(save2_resp["id"], 2);

    // Verify master file updated
    let read_master_2 = std::fs::read_to_string(&master_file)?;
    assert_eq!(read_master_2, modern_content);
    println!("✓ data/master/resume.tex updated to modern content");

    // Check database superseding
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, content_hash, superseded_at FROM master_templates ORDER BY id ASC",
    )
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 2);
    assert!(rows[0].2.is_some(), "Version 1 must have superseded_at set");
    assert!(
        rows[1].2.is_none(),
        "Version 2 must have superseded_at NULL (active)"
    );
    println!("✓ Version 1 superseded_at = {:?}", rows[0].2);
    println!("✓ Version 2 superseded_at = None (active)");

    // --- TEST 7: GET /api/master/history ---
    println!("\n[Test 7] GET /api/master/history...");
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/master/history")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    let history: Value = serde_json::from_slice(&bytes)?;
    let history_arr = history.as_array().unwrap();
    assert_eq!(history_arr.len(), 2);
    assert_eq!(history_arr[0]["id"], 2); // newest first
    assert_eq!(history_arr[1]["id"], 1);
    println!("✓ History returned 2 records in descending order");

    // --- TEST 8: GET /api/master/history/{id} ---
    println!("\n[Test 8] GET /api/master/history/1 & history/2 snapshot retrieval...");
    let router = create_router(state.clone());
    let req1 = Request::builder()
        .uri("/api/master/history/1")
        .body(Body::empty())?;
    let resp1 = router.oneshot(req1).await?;
    assert_eq!(resp1.status(), StatusCode::OK);
    let bytes1 = to_bytes(resp1.into_body(), usize::MAX).await?;
    let v1_detail: Value = serde_json::from_slice(&bytes1)?;
    assert_eq!(v1_detail["content"].as_str().unwrap(), classic_content);
    println!("✓ Retrieved historical snapshot for version 1 from disk snapshot");

    let router = create_router(state.clone());
    let req2 = Request::builder()
        .uri("/api/master/history/2")
        .body(Body::empty())?;
    let resp2 = router.oneshot(req2).await?;
    assert_eq!(resp2.status(), StatusCode::OK);
    let bytes2 = to_bytes(resp2.into_body(), usize::MAX).await?;
    let v2_detail: Value = serde_json::from_slice(&bytes2)?;
    assert_eq!(v2_detail["content"].as_str().unwrap(), modern_content);
    println!("✓ Retrieved historical snapshot for version 2 from disk snapshot");

    // Cleanup test dir
    let _ = std::fs::remove_dir_all(&test_dir);
    println!("\n*** ALL STEP 5 INTEGRATION TESTS PASSED PERFECTLY ***\n");

    Ok(())
}
