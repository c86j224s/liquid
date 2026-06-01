use sqlx::{sqlite::SqlitePool, Row};
use std::fs as std_fs;
use std::path::PathBuf;
use uuid::Uuid;

pub fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("liquid-{}-{}", name, Uuid::new_v4()));
    std_fs::create_dir_all(&dir).unwrap();
    dir
}

pub async fn table_exists(db: &SqlitePool, table: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table)
    .fetch_one(db)
    .await
    .unwrap()
        > 0
}

pub async fn schema_snapshot(db: &SqlitePool) -> Vec<String> {
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(db)
    .await
    .unwrap();

    let mut snapshot = Vec::new();
    for table in tables {
        let pragma = format!("PRAGMA table_info({})", table);
        let columns = sqlx::query(&pragma)
            .fetch_all(db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| {
                let name: String = row.get("name");
                let column_type: String = row.get("type");
                let not_null: i64 = row.get("notnull");
                let default_value: Option<String> = row.get("dflt_value");
                let primary_key: i64 = row.get("pk");
                format!(
                    "{}:{}:{}:{}:{}",
                    name,
                    column_type,
                    not_null,
                    default_value.unwrap_or_else(|| "NULL".to_string()),
                    primary_key
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        snapshot.push(format!("{}|{}", table, columns));
    }
    snapshot
}
