pub(crate) use liquid_files::system_tag_labels_for_task;
use liquid_storage_sqlite::ensure_tag_id;
use sqlx::sqlite::SqlitePool;

#[cfg(test)]
pub(crate) async fn assign_file_tags(
    db: &SqlitePool,
    file_id: i64,
    labels: &[&str],
    source: &str,
) -> Result<(), sqlx::Error> {
    let labels: Vec<String> = labels.iter().map(|label| (*label).to_string()).collect();
    assign_file_tag_labels(db, file_id, &labels, source).await
}

pub(crate) async fn assign_file_tag_labels(
    db: &SqlitePool,
    file_id: i64,
    labels: &[String],
    source: &str,
) -> Result<(), sqlx::Error> {
    if labels.is_empty() {
        return Ok(());
    }

    for label in labels {
        let tag_id = ensure_tag_id(db, label, "system").await?;
        sqlx::query("INSERT OR IGNORE INTO file_tags (file_id, tag_id, source) VALUES (?, ?, ?)")
            .bind(file_id)
            .bind(tag_id)
            .bind(source)
            .execute(db)
            .await?;
    }
    Ok(())
}
