pub mod projects;
pub mod repositories;
pub mod resumes;
pub mod templates;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::Path;

pub async fn init_db(db_path: &Path) -> Result<SqlitePool, anyhow::Error> {
    let connect_options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(5));

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    // Run embedded migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Startup recovery sweep (Section 4)
    sqlx::query(
        "UPDATE resumes SET status = 'cancelled', error_stage = 'server_restart' \
         WHERE status NOT IN ('ready','ready_sparse','github_error','ai_empty_response', \
           'ai_tool_denied','ai_timeout','ai_hung','ai_invalid_json','ai_error','latex_error', \
           'page_limit_error','validation_error','cancelled')",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
