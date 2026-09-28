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
    let test_dir = PathBuf::from(format!("data/test_step15_{}", uuid::Uuid::new_v4()));
    let starter_dir = test_dir.join("templates").join("starter");
    std::fs::create_dir_all(&starter_dir)?;
    std::fs::copy(
        "data/templates/starter/classic.tex",
        starter_dir.join("classic.tex"),
    )?;
    let master_dir = test_dir.join("master");
    std::fs::create_dir_all(&master_dir)?;
    std::fs::copy(
        starter_dir.join("classic.tex"),
        master_dir.join("resume.tex"),
    )?;
    let db_path = test_dir.join("test.db");
    let pool = db::init_db(&db_path).await?;
    let state = AppState::new(
        Config {
            host: "127.0.0.1".into(),
            port: 3000,
            data_dir: test_dir.clone(),
            frontend_dir: PathBuf::from("frontend"),
        },
        pool.clone(),
    );

    let mut ids = Vec::new();
    for number in 0..3 {
        let response = create_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resumes")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        json!({"job_description": format!("Rust engineer job {number}")})
                            .to_string(),
                    ))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
        assert_eq!(
            body["status"], "queued",
            "POST must return before generation"
        );
        ids.push(body["id"].as_i64().unwrap());
    }

    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let mut finished = 0;
        let mut active = 0;
        for id in &ids {
            let row = db::resumes::get_resume_by_id(&pool, *id).await?.unwrap();
            match row.status.as_str() {
                "github_error" => finished += 1,
                "queued" => {}
                "analysing_job" | "retrieving_projects" => active += 1,
                other => anyhow::bail!("Resume {id} failed with {other}: {:?}", row.error_detail),
            }
        }
        assert!(active <= 1, "More than one resume active at once");
        if finished == ids.len() {
            break;
        }
        assert!(Instant::now() < deadline, "Timed out waiting for queue");
        sleep(Duration::from_millis(25)).await;
    }

    for id in &ids {
        let app_dir = test_dir.join("applications").join(id.to_string());
        assert!(app_dir.join("job.txt").exists());
        assert!(app_dir.join("master-snapshot.tex").exists());
    }

    let regenerated = create_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/resumes/{}/regenerate", ids[0]))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(regenerated.status(), StatusCode::CREATED);
    let version: Value =
        serde_json::from_slice(&to_bytes(regenerated.into_body(), usize::MAX).await?)?;
    assert_eq!(version["parent_resume_id"], ids[0]);
    let version_id = version["id"].as_i64().unwrap();
    let version_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let row = db::resumes::get_resume_by_id(&pool, version_id)
            .await?
            .unwrap();
        if row.status == "github_error" {
            break;
        }
        assert!(Instant::now() < version_deadline);
        sleep(Duration::from_millis(25)).await;
    }

    let active = db::resumes::insert_resume(
        &pool,
        None,
        None,
        "active",
        None,
        None,
        "generating_content",
    )
    .await?;
    let delete = create_router(state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/resumes/{}", active.id))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(delete.status(), StatusCode::CONFLICT);
    sqlx::query("UPDATE resumes SET status='cancelled' WHERE id=?")
        .bind(active.id)
        .execute(&pool)
        .await?;

    // A worker failure must become a terminal status with a visible event.
    std::fs::remove_file(master_dir.join("resume.tex"))?;
    let response = create_router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/resumes")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"job_description": "missing template"}).to_string(),
                ))?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await?)?;
    let failed_id = body["id"].as_i64().unwrap();
    let failure_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let row = db::resumes::get_resume_by_id(&pool, failed_id)
            .await?
            .unwrap();
        if row.status == "validation_error" {
            assert_eq!(row.error_stage.as_deref(), Some("master_template"));
            break;
        }
        assert!(
            Instant::now() < failure_deadline,
            "Failed job stayed active"
        );
        sleep(Duration::from_millis(25)).await;
    }
    let event_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let events =
            resumeforge::services::event_bus::get_events_for_resume(&pool, failed_id).await?;
        if events.iter().any(|event| {
            event.event_type == "node_error" && event.payload["stage"] == "validation_error"
        }) {
            break;
        }
        assert!(
            Instant::now() < event_deadline,
            "Failed job did not emit a Canvas error"
        );
        sleep(Duration::from_millis(25)).await;
    }

    let interrupted =
        db::resumes::insert_resume(&pool, None, None, "interrupted", None, None, "queued").await?;
    let completed = db::resumes::get_resume_by_id(&pool, ids[0]).await?.unwrap();
    drop(state);
    drop(pool);
    let recovered_pool = db::init_db(&db_path).await?;
    let recovered = db::resumes::get_resume_by_id(&recovered_pool, interrupted.id)
        .await?
        .unwrap();
    assert_eq!(recovered.status, "cancelled");
    assert_eq!(recovered.error_stage.as_deref(), Some("server_restart"));
    let preserved = db::resumes::get_resume_by_id(&recovered_pool, completed.id)
        .await?
        .unwrap();
    assert_eq!(preserved.status, completed.status);
    recovered_pool.close().await;
    for attempt in 0..20 {
        match std::fs::remove_dir_all(&test_dir) {
            Ok(()) => break,
            Err(err) if attempt < 19 => {
                tracing::debug!("Waiting for SQLite handles to close: {}", err);
                sleep(Duration::from_millis(100)).await;
            }
            Err(err) => return Err(err.into()),
        }
    }
    println!("Step 15 passed: queued POSTs, serial worker, failure status, restart recovery");
    Ok(())
}
