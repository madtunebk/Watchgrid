//! SQL for recordings. No business rules here.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{CameraStorageUsage, Recording, RecordingReason};

use crate::storage::plan::Candidate;

/// A finalized recording to store.
pub struct NewRecording {
    pub id: String,
    pub camera_id: String,
    pub reason: RecordingReason,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: i64,
    pub file_size: i64,
    pub path: String,
    /// Absolute folder the path is relative to; `None` = the default.
    pub root: Option<String>,
    pub codec: String,
    pub width: i32,
    pub height: i32,
}

pub async fn insert(db: &PgPool, r: &NewRecording) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO recordings (id, camera_id, reason, start_time, end_time, duration_ms, file_size, path, codec, width, height, root)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(&r.id)
    .bind(&r.camera_id)
    .bind(reason_name(r.reason))
    .bind(r.start_time)
    .bind(r.end_time)
    .bind(r.duration_ms)
    .bind(r.file_size)
    .bind(&r.path)
    .bind(&r.codec)
    .bind(r.width)
    .bind(r.height)
    .bind(&r.root)
    .execute(db)
    .await
    .map(|_| ())
}

const COLUMNS: &str = "id, camera_id, reason, start_time, end_time, duration_ms, file_size, protected,
    (SELECT COUNT(*) FROM events e WHERE e.recording_id = recordings.id AND e.protected)::int AS protected_by_events";

/// SQL condition: the recording `r` is protected, by hand or by an event.
pub(super) const EFFECTIVELY_PROTECTED: &str = "(r.protected OR EXISTS (SELECT 1 FROM events e WHERE e.recording_id = r.id AND e.protected))";

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    camera_id: String,
    reason: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    duration_ms: i64,
    file_size: i64,
    protected: bool,
    protected_by_events: i32,
}

impl Row {
    fn into_model(self) -> Recording {
        Recording {
            id: self.id,
            camera_id: self.camera_id,
            start_time: self.start_time,
            end_time: Some(self.end_time),
            duration: ((self.duration_ms + 500) / 1000) as u32,
            reason: parse_reason(&self.reason),
            file_size: self.file_size as u64,
            protected: self.protected,
            protected_by_events: self.protected_by_events.max(0) as u32,
            event_ids: Vec::new(),
        }
    }
}

/// Recordings overlapping `[from, to)`, oldest first. Empty `camera_ids` = all.
pub async fn list(db: &PgPool, camera_ids: &[String], from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> sqlx::Result<Vec<Recording>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM recordings
         WHERE (cardinality($1::text[]) = 0 OR camera_id = ANY($1))
           AND ($2::timestamptz IS NULL OR end_time > $2)
           AND ($3::timestamptz IS NULL OR start_time < $3)
         ORDER BY start_time"
    );
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(camera_ids).bind(from).bind(to).fetch_all(db).await?;
    Ok(rows.into_iter().map(Row::into_model).collect())
}

pub async fn get(db: &PgPool, id: &str) -> sqlx::Result<Option<Recording>> {
    let row: Option<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM recordings WHERE id = $1")).bind(id).fetch_optional(db).await?;
    Ok(row.map(Row::into_model))
}

/// Recordings per camera, largest first.
pub async fn usage_by_camera(db: &PgPool) -> sqlx::Result<Vec<CameraStorageUsage>> {
    let rows: Vec<(String, i64, i64, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT camera_id, SUM(file_size)::bigint, COUNT(*), MIN(start_time)
         FROM recordings GROUP BY camera_id ORDER BY 2 DESC, camera_id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(camera_id, bytes, count, oldest)| CameraStorageUsage { camera_id, bytes: bytes as u64, recordings: count as u32, oldest })
        .collect())
}

/// Bytes in recordings that retention never deletes.
pub async fn protected_bytes(db: &PgPool) -> sqlx::Result<u64> {
    let n: i64 = sqlx::query_scalar(&format!("SELECT COALESCE(SUM(r.file_size), 0)::bigint FROM recordings r WHERE {EFFECTIVELY_PROTECTED}")).fetch_one(db).await?;
    Ok(n as u64)
}

/// Unprotected recordings, oldest first.
pub async fn retention_candidates(db: &PgPool) -> sqlx::Result<Vec<Candidate>> {
    // The camera's own limit lives in its recording settings (JSON).
    let rows: Vec<(String, i64, DateTime<Utc>, Option<i32>, Option<String>)> = sqlx::query_as(&format!(
        "SELECT r.id, r.file_size, r.end_time, (c.recording->>'retentionDays')::int, r.root
         FROM recordings r LEFT JOIN cameras c ON c.id = r.camera_id
         WHERE NOT {EFFECTIVELY_PROTECTED} ORDER BY r.start_time, r.id"
    ))
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, bytes, end_time, days, root)| Candidate {
            id,
            bytes: bytes as u64,
            end_time,
            camera_max_days: days.and_then(|d| u32::try_from(d).ok()),
            root,
            volume: None,
        })
        .collect())
}

pub async fn set_protected(db: &PgPool, id: &str, protected: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE recordings SET protected = $2 WHERE id = $1").bind(id).bind(protected).execute(db).await.map(|_| ())
}

/// Saved recordings with their size, protection and all their events.
pub async fn clips_with_events(db: &PgPool, ids: &[String]) -> sqlx::Result<Vec<crate::events::ClipRow>> {
    let rows: Vec<(String, i64, bool, Vec<String>)> = sqlx::query_as(&format!(
        "SELECT r.id, r.file_size, {EFFECTIVELY_PROTECTED},
                COALESCE(array_agg(e.id) FILTER (WHERE e.id IS NOT NULL), '{{}}')
         FROM recordings r LEFT JOIN events e ON e.recording_id = r.id
         WHERE r.id = ANY($1) GROUP BY r.id"
    ))
    .bind(ids)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(id, bytes, protected, event_ids)| crate::events::ClipRow { id, bytes: bytes.max(0) as u64, protected, event_ids }).collect())
}

/// Stored (root, relative path).
pub async fn path(db: &PgPool, id: &str) -> sqlx::Result<Option<(Option<String>, String)>> {
    sqlx::query_as("SELECT root, path FROM recordings WHERE id = $1").bind(id).fetch_optional(db).await
}

fn reason_name(r: RecordingReason) -> &'static str {
    match r {
        RecordingReason::Motion => "motion",
        RecordingReason::Event => "event",
        RecordingReason::Manual => "manual",
        RecordingReason::Continuous => "continuous",
        RecordingReason::Scheduled => "scheduled",
        RecordingReason::Api => "api",
    }
}

fn parse_reason(s: &str) -> RecordingReason {
    match s {
        "motion" => RecordingReason::Motion,
        "event" => RecordingReason::Event,
        "continuous" => RecordingReason::Continuous,
        "scheduled" => RecordingReason::Scheduled,
        "api" => RecordingReason::Api,
        _ => RecordingReason::Manual,
    }
}
