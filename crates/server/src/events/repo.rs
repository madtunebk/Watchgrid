//! SQL for events. No business rules here.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use watchgrid_model::{Event, EventQuery, EventType};

use super::kinds;

/// `thumbnail`: the event's moment inside its clip — the browser shows
/// that frame (no image decoding on the server).
const COLUMNS: &str = "id, camera_id, kind, start_time, end_time, recording_id, source, protected,
    GREATEST(EXTRACT(EPOCH FROM (COALESCE(end_time, now()) - start_time)), 0)::bigint AS duration,
    (SELECT '/api/v1/recordings/' || r.id || '/media#t=' || ROUND(GREATEST(EXTRACT(EPOCH FROM (events.start_time - r.start_time)), 0)::numeric + 0.5, 1)
       FROM recordings r WHERE r.id = events.recording_id) AS thumbnail";

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    camera_id: String,
    kind: String,
    start_time: DateTime<Utc>,
    end_time: Option<DateTime<Utc>>,
    recording_id: Option<String>,
    source: String,
    protected: bool,
    duration: i64,
    thumbnail: Option<String>,
}

impl Row {
    fn into_model(self) -> Event {
        Event {
            id: self.id,
            camera_id: self.camera_id,
            kind: kinds::parse(&self.kind).unwrap_or(EventType::Api),
            start_time: self.start_time,
            end_time: self.end_time,
            duration: self.duration.max(0) as u32,
            recording_id: self.recording_id,
            thumbnail: self.thumbnail,
            protected: self.protected,
            detections: Vec::new(),
            source: self.source,
        }
    }
}

/// Start an event that lasts (outage, recording). No-op if one of this
/// kind is already open for the camera. Returns whether it was created.
pub async fn open(db: &PgPool, camera_id: &str, kind: EventType, at: DateTime<Utc>, source: &str, recording_id: Option<&str>) -> sqlx::Result<bool> {
    open_from(db, camera_id, kind, at, source, recording_id, "watchgrid").await
}

/// Like [`open`], naming the integration that raised the event.
pub async fn open_from(db: &PgPool, camera_id: &str, kind: EventType, at: DateTime<Utc>, source: &str, recording_id: Option<&str>, origin: &str) -> sqlx::Result<bool> {
    let r = sqlx::query(
        "INSERT INTO events (camera_id, kind, start_time, source, recording_id, origin) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (camera_id, kind) WHERE end_time IS NULL DO NOTHING",
    )
    .bind(camera_id)
    .bind(kinds::name(kind))
    .bind(at)
    .bind(source)
    .bind(recording_id)
    .bind(origin)
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Record something that happened at one instant.
pub async fn instant(db: &PgPool, camera_id: &str, kind: EventType, at: DateTime<Utc>, source: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO events (camera_id, kind, start_time, end_time, source) VALUES ($1, $2, $3, $3, $4)")
        .bind(camera_id)
        .bind(kinds::name(kind))
        .bind(at)
        .bind(source)
        .execute(db)
        .await
        .map(|_| ())
}

/// End the open event of this kind. Returns whether there was one.
pub async fn close(db: &PgPool, camera_id: &str, kind: EventType, at: DateTime<Utc>) -> sqlx::Result<bool> {
    let r = sqlx::query("UPDATE events SET end_time = GREATEST($3, start_time) WHERE camera_id = $1 AND kind = $2 AND end_time IS NULL")
        .bind(camera_id)
        .bind(kinds::name(kind))
        .bind(at)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// End an open detection; if it lasted less than `min_secs` (and isn't
/// protected), drop it — too short to matter. Returns whether anything changed.
pub async fn close_detection(db: &PgPool, camera_id: &str, kind: EventType, at: DateTime<Utc>, min_secs: u32) -> sqlx::Result<bool> {
    let closed: Option<(String, f64, bool)> = sqlx::query_as(
        "UPDATE events SET end_time = GREATEST($3, start_time) WHERE camera_id = $1 AND kind = $2 AND end_time IS NULL
         RETURNING id, EXTRACT(EPOCH FROM (end_time - start_time))::float8, protected",
    )
    .bind(camera_id)
    .bind(kinds::name(kind))
    .bind(at)
    .fetch_optional(db)
    .await?;
    let Some((id, secs, protected)) = closed else { return Ok(false) };
    if !protected && secs < f64::from(min_secs) {
        sqlx::query("DELETE FROM events WHERE id = $1").bind(&id).execute(db).await?;
    }
    Ok(true)
}

/// End the open recording event of a camera. Uses the recording's own end
/// time when it was saved; without a file the event loses its recording link.
pub async fn close_recording(db: &PgPool, camera_id: &str, recording_id: Option<&str>, at: DateTime<Utc>, error: Option<&str>) -> sqlx::Result<bool> {
    let r = sqlx::query(
        "UPDATE events e SET
             end_time = GREATEST(COALESCE((SELECT r.end_time FROM recordings r WHERE r.id = $2), $3), e.start_time),
             recording_id = $2,
             source = CASE WHEN $4::text IS NULL THEN e.source ELSE e.source || ' — ended early: ' || $4 END
         WHERE e.camera_id = $1 AND e.kind IN ('manual', 'scheduled') AND e.end_time IS NULL",
    )
    .bind(camera_id)
    .bind(recording_id)
    .bind(at)
    .bind(error)
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Attach a starting event recording to the camera's detections in progress.
pub async fn link_open_detections(db: &PgPool, camera_id: &str, recording_id: &str) -> sqlx::Result<bool> {
    let r = sqlx::query("UPDATE events SET recording_id = $2 WHERE camera_id = $1 AND origin <> 'watchgrid' AND end_time IS NULL")
        .bind(camera_id)
        .bind(recording_id)
        .execute(db)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// The recording was not saved: drop the links to it.
pub async fn unlink_recording(db: &PgPool, recording_id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE events SET recording_id = NULL WHERE recording_id = $1").bind(recording_id).execute(db).await.map(|_| ())
}

/// Delete finished, unprotected events older than `days`. Returns how many.
pub async fn purge_older_than(db: &PgPool, days: u32) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM events WHERE NOT protected AND end_time IS NOT NULL AND end_time < now() - make_interval(days => $1)")
        .bind(days as i32)
        .execute(db)
        .await?;
    Ok(r.rows_affected())
}

/// After a restart: finish recording events left open by the previous run
/// with their recording's end, and detections at their start (their real
/// end is unknown). Outages stay open — the supervisor closes them when
/// the camera connects again.
pub async fn close_stale(db: &PgPool) -> sqlx::Result<u64> {
    let detections = sqlx::query("UPDATE events SET end_time = start_time WHERE origin <> 'watchgrid' AND end_time IS NULL")
        .execute(db)
        .await?
        .rows_affected();
    sqlx::query(
        "UPDATE events e SET
             end_time = COALESCE((SELECT r.end_time FROM recordings r WHERE r.id = e.recording_id), e.start_time),
             recording_id = (SELECT r.id FROM recordings r WHERE r.id = e.recording_id)
         WHERE e.kind IN ('manual', 'scheduled') AND e.end_time IS NULL",
    )
    .execute(db)
    .await
    .map(|r| r.rows_affected() + detections)
}

/// `tz`: IANA zone for the hours-of-day filter.
fn filtered<'a>(select: &str, q: &'a EventQuery, tz: &'a str) -> QueryBuilder<'a, Postgres> {
    let mut b = QueryBuilder::new(select);
    b.push(" FROM events WHERE TRUE");
    if let Some(camera) = &q.camera_id {
        b.push(" AND camera_id = ").push_bind(camera);
    }
    if !q.kinds.is_empty() {
        let names: Vec<&str> = q.kinds.iter().map(|k| kinds::name(*k)).collect();
        b.push(" AND kind = ANY(").push_bind(names).push(")");
    }
    if let Some(from) = q.from {
        b.push(" AND start_time >= ").push_bind(from);
    }
    if let Some(to) = q.to {
        b.push(" AND start_time < ").push_bind(to);
    }
    if let Some((start, end)) = q.hours {
        b.push(" AND ");
        let hour = |b: &mut QueryBuilder<'a, Postgres>| {
            b.push("EXTRACT(HOUR FROM start_time AT TIME ZONE ").push_bind(tz).push(")");
        };
        // `[start, end)` in local hours; wraps past midnight when start > end.
        let join = if start <= end { " AND " } else { " OR " };
        b.push("(");
        hour(&mut b);
        b.push(" >= ").push_bind(i32::from(start)).push(join);
        hour(&mut b);
        b.push(" < ").push_bind(i32::from(end)).push(")");
    }
    if let Some(min) = q.min_duration {
        b.push(" AND EXTRACT(EPOCH FROM (COALESCE(end_time, now()) - start_time)) >= ").push_bind(f64::from(min));
    }
    if q.protected_only {
        b.push(" AND protected");
    }
    b
}

/// A page of matching events, newest first, and the total count.
pub async fn list(db: &PgPool, q: &EventQuery, tz: &str) -> sqlx::Result<(Vec<Event>, u32)> {
    let total: i64 = filtered("SELECT COUNT(*)", q, tz).build_query_scalar().fetch_one(db).await?;
    let mut b = filtered(&format!("SELECT {COLUMNS}"), q, tz);
    b.push(" ORDER BY start_time DESC, id DESC");
    b.push(" LIMIT ").push_bind(i64::from(q.limit.unwrap_or(100).min(1000)));
    b.push(" OFFSET ").push_bind(i64::from(q.offset.unwrap_or(0)));
    let rows: Vec<Row> = b.build_query_as().fetch_all(db).await?;
    Ok((rows.into_iter().map(Row::into_model).collect(), total as u32))
}

pub async fn get(db: &PgPool, id: &str) -> sqlx::Result<Option<Event>> {
    let row: Option<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM events WHERE id = $1")).bind(id).fetch_optional(db).await?;
    Ok(row.map(Row::into_model))
}

/// Ids of the events just before and after this one in time (any camera).
pub async fn neighbours(db: &PgPool, e: &Event) -> sqlx::Result<(Option<String>, Option<String>)> {
    let previous = sqlx::query_scalar(
        "SELECT id FROM events WHERE (start_time, id) < ($1, $2) ORDER BY start_time DESC, id DESC LIMIT 1",
    )
    .bind(e.start_time)
    .bind(&e.id)
    .fetch_optional(db)
    .await?;
    let next = sqlx::query_scalar("SELECT id FROM events WHERE (start_time, id) > ($1, $2) ORDER BY start_time, id LIMIT 1")
        .bind(e.start_time)
        .bind(&e.id)
        .fetch_optional(db)
        .await?;
    Ok((previous, next))
}

pub async fn set_protected(db: &PgPool, id: &str, protected: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE events SET protected = $2 WHERE id = $1").bind(id).bind(protected).execute(db).await.map(|_| ())
}

pub async fn delete(db: &PgPool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM events WHERE id = $1 AND NOT protected").bind(id).execute(db).await.map(|_| ())
}
