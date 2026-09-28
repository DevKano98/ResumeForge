use chrono::NaiveDateTime;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::Row;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("================================================================================");
    println!(
        "ResumeForge - Database & Foreign Key Cascade Verification Spike (Section 12, Step 3)"
    );
    println!("================================================================================");

    // Use a test database file
    let temp_db_dir = PathBuf::from("data").join("temp");
    std::fs::create_dir_all(&temp_db_dir)?;
    let test_db_path = temp_db_dir.join("test_fk_cascade.db");
    if test_db_path.exists() {
        let _ = std::fs::remove_file(&test_db_path);
    }

    println!(
        "1. Connecting to SQLite test database at {:?}",
        test_db_path
    );
    let connect_options = SqliteConnectOptions::new()
        .filename(&test_db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    println!("2. Running migrations via sqlx::migrate!(\"./migrations\")...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    println!("   Migrations applied successfully!");

    println!("3. Verifying PRAGMA foreign_keys value across connections...");
    let fk_status: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await?;
    println!("   PRAGMA foreign_keys = {} (expected 1)", fk_status);
    assert_eq!(
        fk_status, 1,
        "Foreign keys must be enabled on every connection"
    );

    println!("4. Inserting prerequisites: repository and project for resume_projects FK...");
    let repo_id = sqlx::query(
        r#"
        INSERT INTO repositories (github_repo_id, owner, name, url, visibility)
        VALUES ('test_gh_1', 'octocat', 'hello-world', 'https://github.com/octocat/hello-world', 'public')
        "#
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let project_id = sqlx::query(
        r#"
        INSERT INTO projects (repository_id, name)
        VALUES (?, 'TestProject')
        "#,
    )
    .bind(repo_id)
    .execute(&pool)
    .await?
    .last_insert_rowid();
    println!(
        "   Created repository id={}, project id={}",
        repo_id, project_id
    );

    println!("5. Inserting a resume row and testing chrono round-trip on created_at...");
    let resume_id = sqlx::query(
        r#"
        INSERT INTO resumes (job_description, status)
        VALUES ('Looking for a Senior Rust Engineer', 'queued')
        "#,
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();
    println!("   Created resume id={}", resume_id);

    // Chrono round-trip test on datetime('now') TEXT default
    let row = sqlx::query("SELECT id, created_at, status FROM resumes WHERE id = ?")
        .bind(resume_id)
        .fetch_one(&pool)
        .await?;

    let created_at_str: String = row.get("created_at");
    println!("   Raw created_at TEXT from SQLite: '{}'", created_at_str);

    // Decode as chrono::NaiveDateTime
    let created_at_naive: NaiveDateTime = row.get("created_at");
    println!(
        "   Decoded as chrono::NaiveDateTime: {:?}",
        created_at_naive
    );

    println!("6. Inserting dependent rows in resume_projects and generation_events...");
    sqlx::query(
        r#"
        INSERT INTO resume_projects (resume_id, project_id, relevance_score, selected)
        VALUES (?, ?, 0.95, 1)
        "#,
    )
    .bind(resume_id)
    .bind(project_id)
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO generation_events (resume_id, seq, node, event_type, payload_json)
        VALUES (?, 1, 'jd_analyzer', 'node_done', '{"skills":["Rust","SQLite"]}')
        "#,
    )
    .bind(resume_id)
    .execute(&pool)
    .await?;

    // Verify both rows exist
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
    println!(
        "   Verified dependent rows exist: resume_projects count={}, generation_events count={}",
        count_rp, count_ge
    );
    assert_eq!(count_rp, 1);
    assert_eq!(count_ge, 1);

    println!("7. Verifying FK constraint rejection on non-existent parent ID...");
    let invalid_insert_res = sqlx::query(
        r#"
        INSERT INTO generation_events (resume_id, seq, node, event_type, payload_json)
        VALUES (999999, 1, 'test', 'node_done', '{}')
        "#,
    )
    .execute(&pool)
    .await;
    match invalid_insert_res {
        Err(e) => {
            println!(
                "   [PASS] Insertion with invalid resume_id was rejected: {}",
                e
            );
        }
        Ok(_) => {
            panic!("FAILED: Foreign key violation was NOT enforced by SQLite!");
        }
    }

    println!("8. Deleting parent row from resumes (id={})...", resume_id);
    let delete_res = sqlx::query("DELETE FROM resumes WHERE id = ?")
        .bind(resume_id)
        .execute(&pool)
        .await?;
    println!(
        "   Deleted {} row(s) from resumes",
        delete_res.rows_affected()
    );
    assert_eq!(delete_res.rows_affected(), 1);

    println!("9. Asserting cascade deletion in child tables...");
    let remaining_rp: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM resume_projects WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;
    let remaining_ge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM generation_events WHERE resume_id = ?")
            .bind(resume_id)
            .fetch_one(&pool)
            .await?;

    println!(
        "   Remaining resume_projects count for resume {}: {}",
        resume_id, remaining_rp
    );
    println!(
        "   Remaining generation_events count for resume {}: {}",
        resume_id, remaining_ge
    );

    assert_eq!(
        remaining_rp, 0,
        "CASCADE DELETE failed: resume_projects row still exists!"
    );
    assert_eq!(
        remaining_ge, 0,
        "CASCADE DELETE failed: generation_events row still exists!"
    );

    println!("\n>>> SUCCESS: All tests passed! Foreign keys and cascade deletion are fully operational. <<<\n");

    // Clean up temporary db file
    drop(pool);
    let _ = std::fs::remove_file(&test_db_path);

    Ok(())
}
