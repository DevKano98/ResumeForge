use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use resumeforge::config::Config;
use resumeforge::db;
use resumeforge::routes::create_router;
use resumeforge::services::{latex, pdf};
use resumeforge::state::AppState;
use std::path::PathBuf;
use tower::util::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!(
        "=== ResumeForge Step 6 Integration Test: Tectonic Compilation, PDF Metrics & Serving ==="
    );

    // Use clean test directory
    let test_dir = PathBuf::from("data/test_step6");
    if test_dir.exists() {
        let _ = std::fs::remove_dir_all(&test_dir);
    }
    std::fs::create_dir_all(&test_dir)?;

    let db_path = test_dir.join("test.db");
    let pool = db::init_db(&db_path).await?;

    let config = Config {
        host: "127.0.0.1".to_string(),
        port: 3000,
        data_dir: test_dir.clone(),
        frontend_dir: PathBuf::from("frontend"),
    };

    let state = AppState::new(config, pool);

    // --- TEST 1: Tectonic compilation service ---
    println!("\n[Test 1] Testing latex::compile_latex_content...");
    let valid_latex = r#"\documentclass{article}
\begin{document}
Hello ResumeForge PDF!
\end{document}"#;

    let compile_dir = test_dir.join("compiles");
    let generated_pdf = compile_dir.join("test1.pdf");
    let res = latex::compile_latex_content(valid_latex, &compile_dir, Some(&generated_pdf)).await?;
    assert!(res.success, "Compilation must succeed for valid LaTeX");
    assert!(generated_pdf.exists(), "Generated PDF must exist on disk");
    println!("✓ Generated test PDF at {:?}", generated_pdf);

    // --- TEST 2: PDF metrics via services::pdf ---
    println!("\n[Test 2] Testing pdf::inspect_pdf on generated PDF...");
    let metrics = pdf::inspect_pdf(&generated_pdf)?;
    println!("Metrics for short 1-page document: {:?}", metrics);
    assert_eq!(metrics.page_count, 1);
    assert!(metrics.content_density_pct > 0.0 && metrics.content_density_pct < 60.0);
    assert!(
        metrics.is_sparse,
        "Short one-liner document should be flagged is_sparse = true"
    );
    println!(
        "✓ Correctly computed page_count=1, density={:.1}%, is_sparse=true",
        metrics.content_density_pct
    );

    // Test on starter template (classic.tex)
    let classic_src = std::fs::read_to_string("data/templates/starter/classic.tex")?;
    let classic_pdf = compile_dir.join("classic.pdf");
    let res_classic =
        latex::compile_latex_content(&classic_src, &compile_dir, Some(&classic_pdf)).await?;
    assert!(res_classic.success);
    let classic_metrics = pdf::inspect_pdf(&classic_pdf)?;
    println!("Classic template metrics: {:?}", classic_metrics);
    assert_eq!(classic_metrics.page_count, 1);
    println!(
        "✓ Classic template: page_count={}, density={:.1}%",
        classic_metrics.page_count, classic_metrics.content_density_pct
    );

    // --- TEST 3: Master PDF serving endpoint (GET /api/template/pdf) ---
    println!("\n[Test 3] Testing GET /api/template/pdf...");
    // 3a: When master does not exist -> 404
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template/pdf")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    println!("✓ GET /api/template/pdf returns 404 when master doesn't exist");

    // 3b: Save classic as master
    let master_dir = test_dir.join("master");
    std::fs::create_dir_all(&master_dir)?;
    std::fs::write(master_dir.join("resume.tex"), &classic_src)?;

    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/template/pdf")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/pdf"
    );

    let pdf_bytes = to_bytes(resp.into_body(), usize::MAX).await?;
    assert!(
        pdf_bytes.starts_with(b"%PDF-"),
        "Response must start with %PDF- header"
    );
    println!(
        "✓ GET /api/template/pdf served {} bytes of valid PDF",
        pdf_bytes.len()
    );

    // --- TEST 4: Resume application PDF and TeX serving ---
    println!("\n[Test 4] Testing GET /api/resumes/:id/pdf and :id/tex...");
    let app_id = "test-resume-app-42";
    let app_dir = test_dir.join("applications").join(app_id);
    std::fs::create_dir_all(&app_dir)?;
    std::fs::copy(&classic_pdf, app_dir.join("resume.pdf"))?;
    std::fs::write(app_dir.join("resume.tex"), &classic_src)?;

    // 4a: Download PDF
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri(format!("/api/resumes/{}/pdf", app_id))
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/pdf"
    );
    let downloaded_pdf = to_bytes(resp.into_body(), usize::MAX).await?;
    assert!(downloaded_pdf.starts_with(b"%PDF-"));
    println!("✓ GET /api/resumes/{}/pdf served valid PDF", app_id);

    // 4b: Download TeX
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri(format!("/api/resumes/{}/tex", app_id))
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/x-tex"
    );
    let downloaded_tex = to_bytes(resp.into_body(), usize::MAX).await?;
    assert_eq!(String::from_utf8_lossy(&downloaded_tex), classic_src);
    println!(
        "✓ GET /api/resumes/{}/tex served matching LaTeX source",
        app_id
    );

    // 4c: Nonexistent ID -> 404
    let router = create_router(state.clone());
    let req = Request::builder()
        .uri("/api/resumes/nonexistent-xyz/pdf")
        .body(Body::empty())?;
    let resp = router.oneshot(req).await?;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    println!("✓ GET /api/resumes/nonexistent-xyz/pdf correctly returns 404");

    // Cleanup
    let _ = std::fs::remove_dir_all(&test_dir);
    println!("\n*** ALL STEP 6 TESTS PASSED PERFECTLY ***\n");

    Ok(())
}
