//! SQL for cameras. No business rules here.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::types::Json;
use watchgrid_model::{Camera, CameraInput, CameraStatus, MotionSettings, OnvifConfig, RecordingSettings, Stream};

const COLUMNS: &str = "id, name, description, location, enabled, host, username, main_stream_url, sub_stream_url, onvif, recording, motion, created_at";

#[derive(sqlx::FromRow)]
pub struct CameraRow {
    id: String,
    name: String,
    description: String,
    location: String,
    enabled: bool,
    host: String,
    username: String,
    main_stream_url: String,
    sub_stream_url: Option<String>,
    onvif: Option<Json<OnvifConfig>>,
    recording: Json<RecordingSettings>,
    motion: Json<MotionSettings>,
    created_at: DateTime<Utc>,
}

impl CameraRow {
    /// Stored configuration only; live fields start "not connected" until
    /// the camera supervisor reports otherwise.
    pub fn into_model(self) -> Camera {
        Camera {
            id: self.id,
            name: self.name,
            description: self.description,
            location: self.location,
            enabled: self.enabled,
            status: CameraStatus::Offline,
            host: self.host,
            username: self.username,
            main_stream: Stream::unprobed(self.main_stream_url),
            sub_stream: self.sub_stream_url.map(Stream::unprobed),
            onvif: self.onvif.map(|o| o.0),
            recording: self.recording.0,
            motion: self.motion.0,
            recording_active: false,
            recording_reason: None,
            motion_active: false,
            motion_fallback: false,
            software_motion: None,
            last_event: None,
            storage_used: None,
            connected_since: None,
            created_at: self.created_at,
        }
    }
}

/// What to do with a stored secret on update.
pub enum Secret {
    Keep,
    Set(Vec<u8>),
    Clear,
}

impl Secret {
    fn parts(&self) -> (bool, Option<&[u8]>) {
        match self {
            Secret::Keep => (true, None),
            Secret::Set(v) => (false, Some(v)),
            Secret::Clear => (false, None),
        }
    }
}

/// ONVIF settings as stored: never with the password (that is encrypted separately).
/// Id, ONVIF config and encrypted ONVIF password of the camera using this ONVIF URL.
pub async fn onvif_by_url(db: &PgPool, url: &str) -> sqlx::Result<Option<(String, Json<OnvifConfig>, Option<Vec<u8>>)>> {
    sqlx::query_as("SELECT id, onvif, onvif_password_enc FROM cameras WHERE onvif->>'url' = $1 ORDER BY created_at LIMIT 1")
        .bind(url)
        .fetch_optional(db)
        .await
}

fn public_onvif(o: &Option<OnvifConfig>) -> Option<Json<OnvifConfig>> {
    o.as_ref().map(|o| Json(OnvifConfig { url: o.url.trim().into(), username: o.username.trim().into(), password: None }))
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23505"))
}

/// A unique violation on the camera name (not on the id).
pub fn is_duplicate_name(e: &sqlx::Error) -> bool {
    is_unique_violation(e) && matches!(e, sqlx::Error::Database(d) if d.constraint() == Some("cameras_name_unique"))
}

pub async fn list(db: &PgPool) -> sqlx::Result<Vec<CameraRow>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM cameras ORDER BY created_at, id")).fetch_all(db).await
}

pub async fn get(db: &PgPool, id: &str) -> sqlx::Result<Option<CameraRow>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM cameras WHERE id = $1")).bind(id).fetch_optional(db).await
}

/// Whether `id` is used, now or in history: a deleted camera's recordings
/// and events keep its id, and a new camera must never inherit them.
pub async fn id_taken(db: &PgPool, id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM cameras WHERE id = $1)
             OR EXISTS (SELECT 1 FROM recordings WHERE camera_id = $1)
             OR EXISTS (SELECT 1 FROM events WHERE camera_id = $1)",
    )
    .bind(id)
    .fetch_one(db)
    .await
}

pub async fn insert(db: &PgPool, id: &str, i: &CameraInput, password: Option<Vec<u8>>, onvif_password: Option<Vec<u8>>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO cameras (id, name, description, location, enabled, host, username, password_enc,
             main_stream_url, sub_stream_url, onvif, onvif_password_enc, recording, motion)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(id)
    .bind(i.name.trim())
    .bind(i.description.trim())
    .bind(i.location.trim())
    .bind(i.enabled)
    .bind(i.host.trim())
    .bind(i.username.trim())
    .bind(password)
    .bind(i.main_stream_url.trim())
    .bind(i.sub_stream_url.as_deref().map(str::trim))
    .bind(public_onvif(&i.onvif))
    .bind(onvif_password)
    .bind(Json(&i.recording))
    .bind(Json(&i.motion))
    .execute(db)
    .await
    .map(|_| ())
}

/// Returns false when no camera has this id.
pub async fn update(db: &PgPool, id: &str, i: &CameraInput, password: Secret, onvif_password: Secret) -> sqlx::Result<bool> {
    let (keep_pw, pw) = password.parts();
    let (keep_onvif, onvif_pw) = onvif_password.parts();
    let done = sqlx::query(
        "UPDATE cameras SET name = $2, description = $3, location = $4, enabled = $5, host = $6, username = $7,
             password_enc = CASE WHEN $8 THEN password_enc ELSE $9 END,
             main_stream_url = $10, sub_stream_url = $11, onvif = $12,
             onvif_password_enc = CASE WHEN $13 THEN onvif_password_enc ELSE $14 END,
             recording = $15, motion = $16, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(i.name.trim())
    .bind(i.description.trim())
    .bind(i.location.trim())
    .bind(i.enabled)
    .bind(i.host.trim())
    .bind(i.username.trim())
    .bind(keep_pw)
    .bind(pw)
    .bind(i.main_stream_url.trim())
    .bind(i.sub_stream_url.as_deref().map(str::trim))
    .bind(public_onvif(&i.onvif))
    .bind(keep_onvif)
    .bind(onvif_pw)
    .bind(Json(&i.recording))
    .bind(Json(&i.motion))
    .execute(db)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn set_enabled(db: &PgPool, id: &str, enabled: bool) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE cameras SET enabled = $2, updated_at = now() WHERE id = $1").bind(id).bind(enabled).execute(db).await?;
    Ok(done.rows_affected() == 1)
}

pub async fn delete(db: &PgPool, id: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM cameras WHERE id = $1").bind(id).execute(db).await?;
    Ok(done.rows_affected() == 1)
}

/// Encrypted stream password of a camera, if any. `None` = no such camera.
pub async fn onvif_password_enc(db: &PgPool, id: &str) -> sqlx::Result<Option<Option<Vec<u8>>>> {
    sqlx::query_scalar("SELECT onvif_password_enc FROM cameras WHERE id = $1").bind(id).fetch_optional(db).await
}

pub async fn password_enc(db: &PgPool, id: &str) -> sqlx::Result<Option<Option<Vec<u8>>>> {
    sqlx::query_scalar("SELECT password_enc FROM cameras WHERE id = $1").bind(id).fetch_optional(db).await
}
