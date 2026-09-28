use resumeforge::db;
use sqlx::Row;
use std::path::{Path, PathBuf};

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
async fn test_startup_recovery_sweep_cancels_in_flight_resumes() -> anyhow::Result<()> {
    let tmp = TestDir::new("recovery");
    let db_path = tmp.path().join("test_recovery.db");

    // Initialize DB first time
    let pool = db::init_db(&db_path).await?;

    // Insert an active in-flight resume and a completed resume
    let in_flight_id: i64 = sqlx::query(
        "INSERT INTO resumes (job_description, status) VALUES ('In flight job', 'generating_content')",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let analyzing_id: i64 = sqlx::query(
        "INSERT INTO resumes (job_description, status) VALUES ('Analyzing job', 'analysing_job')",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let completed_id: i64 = sqlx::query(
        "INSERT INTO resumes (job_description, status, page_count) VALUES ('Completed job', 'ready', 1)",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let failed_id: i64 = sqlx::query(
        "INSERT INTO resumes (job_description, status) VALUES ('Timed out job', 'ai_timeout')",
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    // Close first pool to simulate server process exiting
    pool.close().await;

    // Re-initialize DB (simulating server restart)
    let pool2 = db::init_db(&db_path).await?;

    // Verify in-flight resumes were updated to 'cancelled' with error_stage = 'server_restart'
    let row1 = sqlx::query("SELECT status, error_stage FROM resumes WHERE id = ?")
        .bind(in_flight_id)
        .fetch_one(&pool2)
        .await?;
    let status1: String = row1.get("status");
    let stage1: Option<String> = row1.get("error_stage");
    assert_eq!(status1, "cancelled");
    assert_eq!(stage1, Some("server_restart".to_string()));

    let row2 = sqlx::query("SELECT status, error_stage FROM resumes WHERE id = ?")
        .bind(analyzing_id)
        .fetch_one(&pool2)
        .await?;
    let status2: String = row2.get("status");
    assert_eq!(status2, "cancelled");

    // Verify terminal states were preserved
    let row3 = sqlx::query("SELECT status FROM resumes WHERE id = ?")
        .bind(completed_id)
        .fetch_one(&pool2)
        .await?;
    let status3: String = row3.get("status");
    assert_eq!(status3, "ready", "Terminal status 'ready' must not be changed");

    let row4 = sqlx::query("SELECT status FROM resumes WHERE id = ?")
        .bind(failed_id)
        .fetch_one(&pool2)
        .await?;
    let status4: String = row4.get("status");
    assert_eq!(status4, "ai_timeout", "Terminal status 'ai_timeout' must not be changed");

    Ok(())
}
