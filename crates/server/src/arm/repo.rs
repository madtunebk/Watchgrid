//! `armed_cameras`: one row per armed camera, with its settings from before.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::types::Json;
use watchgrid_model::ArmSettings;

#[derive(sqlx::FromRow)]
pub struct Row {
    pub camera_id: String,
    pub armed_at: DateTime<Utc>,
    pub before: Json<ArmSettings>,
}

pub async fn list(db: &PgPool) -> sqlx::Result<Vec<Row>> {
    sqlx::query_as("SELECT camera_id, armed_at, before FROM armed_cameras ORDER BY armed_at, camera_id").fetch_all(db).await
}

pub async fn insert(db: &PgPool, camera_id: &str, before: ArmSettings) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO armed_cameras (camera_id, before) VALUES ($1, $2) ON CONFLICT (camera_id) DO NOTHING")
        .bind(camera_id)
        .bind(Json(before))
        .execute(db)
        .await
        .map(|_| ())
}

pub async fn delete(db: &PgPool, camera_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM armed_cameras WHERE camera_id = $1").bind(camera_id).execute(db).await.map(|_| ())
}
