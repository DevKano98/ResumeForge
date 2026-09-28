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
async fn test_database_foreign_keys_and_cascade_delete() -> anyhow::Result<()> {
    let tmp = TestDir::new("fk");
    let db_path = tmp.path().join("test_fk.db");

    let pool = db::init_db(&db_path).await?;

    // 1. Verify PRAGMA foreign_keys = 1
    let fk_status: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
        .fetch_one(&pool)
        .await?;
    assert_eq!(fk_status, 1, "Foreign keys must be enabled on SQLite pool");

    // 2. Insert repository, project, and evidence
    let repo_id = sqlx::query(
        r#"
        INSERT INTO repositories (github_repo_id, owner, name, url, visibility)
        VALUES ('gh_test_1', 'octocat', 'repo-one', 'https://github.com/octocat/repo-one', 'public')
        "#,
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let project_id = sqlx::query(
        r#"
        INSERT INTO projects (repository_id, name)
        VALUES (?, 'ProjectOne')
        "#,
    )
    .bind(repo_id)
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let evidence_id = sqlx::query(
        r#"
        INSERT INTO project_evidence (project_id, claim, source_file)
        VALUES (?, 'Implemented zero-copy stream parser', 'src/lib.rs')
        "#,
    )
    .bind(project_id)
    .execute(&pool)
    .await?
    .last_insert_rowid();

    // 3. Insert parent resume and child resume
    let parent_id = sqlx::query(
        r#"
        INSERT INTO resumes (job_description, status)
        VALUES ('Senior Systems Engineer JD', 'ready')
        "#,
    )
    .execute(&pool)
    .await?
    .last_insert_rowid();

    let child_id = sqlx::query(
        r#"
        INSERT INTO resumes (job_description, status, parent_resume_id)
        VALUES ('Regenerated Systems Engineer JD', 'queued', ?)
        "#,
    )
    .bind(parent_id)
    .execute(&pool)
    .await?
    .last_insert_rowid();

    // 4. Insert resume_project and generation_event
    sqlx::query(
        r#"
        INSERT INTO resume_projects (resume_id, project_id, relevance_score)
        VALUES (?, ?, 0.95)
        "#,
    )
    .bind(child_id)
    .bind(project_id)
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO generation_events (resume_id, seq, node, event_type, payload_json)
        VALUES (?, 1, 'jd_analysis', 'command_started', '{"msg":"Starting"}')
        "#,
    )
    .bind(child_id)
    .execute(&pool)
    .await?;

    // 5. Test SET NULL on parent_resume_id deletion
    sqlx::query("DELETE FROM resumes WHERE id = ?")
        .bind(parent_id)
        .execute(&pool)
        .await?;

    let child_row = sqlx::query("SELECT parent_resume_id FROM resumes WHERE id = ?")
        .bind(child_id)
        .fetch_one(&pool)
        .await?;
    let parent_ref: Option<i64> = child_row.get("parent_resume_id");
    assert_eq!(
        parent_ref, None,
        "parent_resume_id must become NULL when parent resume is deleted"
    );

    // 6. Test CASCADE DELETE on resume deletion
    sqlx::query("DELETE FROM resumes WHERE id = ?")
        .bind(child_id)
        .execute(&pool)
        .await?;

    let rp_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resume_projects WHERE resume_id = ?")
        .bind(child_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(rp_count, 0, "resume_projects must be deleted when resume is deleted");

    let ge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM generation_events WHERE resume_id = ?")
        .bind(child_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(ge_count, 0, "generation_events must be deleted when resume is deleted");

    // 7. Test CASCADE DELETE on repository deletion
    sqlx::query("DELETE FROM repositories WHERE id = ?")
        .bind(repo_id)
        .execute(&pool)
        .await?;

    let proj_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(proj_count, 0, "projects must be cascade deleted with repository");

    let ev_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_evidence WHERE id = ?")
        .bind(evidence_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(ev_count, 0, "project_evidence must be cascade deleted with repository");

    Ok(())
}
