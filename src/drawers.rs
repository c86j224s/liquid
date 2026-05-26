use crate::models::{DrawerAssignment, DrawerInfo, DrawerPayload, FileMetadata};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

pub(crate) async fn list_drawers(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sql = r#"
        SELECT
            drawers.id,
            drawers.name,
            drawers.description,
            drawers.created_at,
            COUNT(files.id) AS file_count
        FROM drawers
        LEFT JOIN files
            ON files.drawer_id = drawers.id
            AND files.status = 'published'
        GROUP BY drawers.id, drawers.name, drawers.description, drawers.created_at
        ORDER BY drawers.name COLLATE NOCASE ASC
    "#;
    match sqlx::query_as::<_, DrawerInfo>(sql)
        .fetch_all(&state.db)
        .await
    {
        Ok(drawers) => Json(drawers).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn create_drawer(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DrawerPayload>,
) -> impl IntoResponse {
    let name = payload.name.trim();
    if name.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match sqlx::query("INSERT INTO drawers (name, description) VALUES (?, ?)")
        .bind(name)
        .bind(payload.description)
        .execute(&state.db)
        .await
    {
        Ok(result) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": result.last_insert_rowid() })),
        )
            .into_response(),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            StatusCode::CONFLICT.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn update_drawer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<DrawerPayload>,
) -> impl IntoResponse {
    let name = payload.name.trim();
    if name.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match sqlx::query("UPDATE drawers SET name = ?, description = ? WHERE id = ?")
        .bind(name)
        .bind(payload.description)
        .bind(id)
        .execute(&state.db)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            StatusCode::CONFLICT.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn delete_drawer(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if sqlx::query("UPDATE files SET drawer_id = NULL WHERE drawer_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    match sqlx::query("DELETE FROM drawers WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        Ok(result) if result.rows_affected() > 0 => match tx.commit().await {
            Ok(_) => StatusCode::OK,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        },
        Ok(_) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(crate) async fn update_file_drawer(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Json(payload): Json<DrawerAssignment>,
) -> impl IntoResponse {
    let file = match sqlx::query_as::<_, FileMetadata>("SELECT * FROM files WHERE filename = ?")
        .bind(&filename)
        .fetch_one(&state.db)
        .await
    {
        Ok(file) => file,
        Err(sqlx::Error::RowNotFound) => return StatusCode::NOT_FOUND,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    let Some(drawer_id) = payload.drawer_id else {
        return match sqlx::query("UPDATE files SET drawer_id = NULL WHERE filename = ?")
            .bind(&filename)
            .execute(&state.db)
            .await
        {
            Ok(_) => StatusCode::OK,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
    };

    if file.status != "published" {
        return StatusCode::BAD_REQUEST;
    }

    let drawer_exists =
        match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM drawers WHERE id = ?")
            .bind(drawer_id)
            .fetch_one(&state.db)
            .await
        {
            Ok(count) => count > 0,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
        };

    if !drawer_exists {
        return StatusCode::NOT_FOUND;
    }

    match sqlx::query("UPDATE files SET drawer_id = ? WHERE filename = ?")
        .bind(drawer_id)
        .bind(&filename)
        .execute(&state.db)
        .await
    {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::setup_db;
    use crate::models::DrawerAssignment;
    use crate::test_support::{response_status, temp_test_dir, test_state};
    use axum::{
        extract::{Path, State},
        http::StatusCode,
        Json,
    };
    use std::fs as std_fs;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_drawer_delete_clears_only_file_drawer_id() {
        let dir = temp_test_dir("drawer-delete");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        sqlx::query("INSERT INTO drawers (name) VALUES ('Shelf')")
            .execute(&db)
            .await
            .unwrap();
        let drawer_id = sqlx::query_scalar::<_, i64>("SELECT id FROM drawers WHERE name = 'Shelf'")
            .fetch_one(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (filename, original_name, file_type, status, drawer_id) VALUES ('published.md', 'Published', 'md', 'published', ?)")
            .bind(drawer_id)
            .execute(&db).await.unwrap();

        let status = delete_drawer(State(state), Path(drawer_id)).await;
        assert_eq!(response_status(status), StatusCode::OK);

        let file_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM files WHERE filename = 'published.md'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let drawer_id: Option<i64> =
            sqlx::query_scalar("SELECT drawer_id FROM files WHERE filename = 'published.md'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(file_count, 1);
        assert_eq!(drawer_id, None);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn test_drawer_assignment_rules() {
        let dir = temp_test_dir("drawer-rules");
        let db = setup_db(&dir).await.unwrap();
        let state = test_state(db.clone(), dir.join("uploads"));

        sqlx::query("INSERT INTO drawers (name) VALUES ('One'), ('Two')")
            .execute(&db)
            .await
            .unwrap();
        let one = sqlx::query_scalar::<_, i64>("SELECT id FROM drawers WHERE name = 'One'")
            .fetch_one(&db)
            .await
            .unwrap();
        let two = sqlx::query_scalar::<_, i64>("SELECT id FROM drawers WHERE name = 'Two'")
            .fetch_one(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (filename, original_name, file_type, status) VALUES ('draft.md', 'Draft', 'md', 'draft'), ('archived.md', 'Archived', 'md', 'archived'), ('published.md', 'Published', 'md', 'published')")
            .execute(&db).await.unwrap();

        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("draft.md".to_string()),
                    Json(DrawerAssignment {
                        drawer_id: Some(one)
                    })
                )
                .await
            ),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("archived.md".to_string()),
                    Json(DrawerAssignment {
                        drawer_id: Some(one)
                    })
                )
                .await
            ),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("published.md".to_string()),
                    Json(DrawerAssignment {
                        drawer_id: Some(999_999)
                    })
                )
                .await
            ),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("published.md".to_string()),
                    Json(DrawerAssignment {
                        drawer_id: Some(one)
                    })
                )
                .await
            ),
            StatusCode::OK
        );
        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("published.md".to_string()),
                    Json(DrawerAssignment {
                        drawer_id: Some(two)
                    })
                )
                .await
            ),
            StatusCode::OK
        );

        let assigned: Option<i64> =
            sqlx::query_scalar("SELECT drawer_id FROM files WHERE filename = 'published.md'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(assigned, Some(two));

        assert_eq!(
            response_status(
                update_file_drawer(
                    State(Arc::clone(&state)),
                    Path("published.md".to_string()),
                    Json(DrawerAssignment { drawer_id: None })
                )
                .await
            ),
            StatusCode::OK
        );
        let cleared: Option<i64> =
            sqlx::query_scalar("SELECT drawer_id FROM files WHERE filename = 'published.md'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(cleared, None);

        db.close().await;
        std_fs::remove_dir_all(dir).unwrap();
    }
}
