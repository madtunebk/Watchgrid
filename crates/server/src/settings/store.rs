//! Settings documents stored as JSON in PostgreSQL, one per section key.

use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use sqlx::types::Json;

/// A stored section, or `None` if it was never saved.
pub async fn load<T: DeserializeOwned + Send + Unpin + 'static>(db: &PgPool, key: &str) -> sqlx::Result<Option<T>> {
    let row: Option<Json<T>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1").bind(key).fetch_optional(db).await?;
    Ok(row.map(|j| j.0))
}

pub async fn save<T: Serialize + Sync>(db: &PgPool, key: &str, value: &T) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(Json(value))
    .execute(db)
    .await
    .map(|_| ())
}
