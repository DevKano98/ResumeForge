use crate::models::template::MasterTemplate;
use sqlx::SqlitePool;

pub async fn get_current_master_template(
    pool: &SqlitePool,
) -> Result<Option<MasterTemplate>, sqlx::Error> {
    sqlx::query_as::<_, MasterTemplate>(
        "SELECT id, content_hash, file_path, is_starter_template, adapted_from_paste, created_at, superseded_at \
         FROM master_templates \
         WHERE superseded_at IS NULL \
         ORDER BY id DESC \
         LIMIT 1"
    )
    .fetch_optional(pool)
    .await
}

pub async fn save_master_template(
    pool: &SqlitePool,
    content_hash: &str,
    file_path: &str,
    is_starter: bool,
    adapted: bool,
) -> Result<MasterTemplate, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Mark previous active templates as superseded
    sqlx::query(
        "UPDATE master_templates SET superseded_at = datetime('now') WHERE superseded_at IS NULL",
    )
    .execute(&mut *tx)
    .await?;

    let is_starter_val: i64 = if is_starter { 1 } else { 0 };
    let adapted_val: i64 = if adapted { 1 } else { 0 };

    let record = sqlx::query_as::<_, MasterTemplate>(
        "INSERT INTO master_templates (content_hash, file_path, is_starter_template, adapted_from_paste, created_at, superseded_at) \
         VALUES (?, ?, ?, ?, datetime('now'), NULL) \
         RETURNING id, content_hash, file_path, is_starter_template, adapted_from_paste, created_at, superseded_at"
    )
    .bind(content_hash)
    .bind(file_path)
    .bind(is_starter_val)
    .bind(adapted_val)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(record)
}

pub async fn list_master_history(pool: &SqlitePool) -> Result<Vec<MasterTemplate>, sqlx::Error> {
    sqlx::query_as::<_, MasterTemplate>(
        "SELECT id, content_hash, file_path, is_starter_template, adapted_from_paste, created_at, superseded_at \
         FROM master_templates \
         ORDER BY id DESC"
    )
    .fetch_all(pool)
    .await
}

pub async fn get_master_template_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<MasterTemplate>, sqlx::Error> {
    sqlx::query_as::<_, MasterTemplate>(
        "SELECT id, content_hash, file_path, is_starter_template, adapted_from_paste, created_at, superseded_at \
         FROM master_templates \
         WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}
