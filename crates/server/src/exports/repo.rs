//! SQL for export destinations and jobs.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{AutoUpload, ExportJob, ExportKind, ExportState, ExportTarget, ExportTargetSettings};

pub fn kind_name(k: ExportKind) -> &'static str {
    match k {
        ExportKind::GoogleDrive => "google_drive",
        ExportKind::S3 => "s3",
        ExportKind::Nextcloud => "nextcloud",
        ExportKind::Dropbox => "dropbox",
    }
}

fn parse_kind(s: &str) -> ExportKind {
    match s {
        "google_drive" => ExportKind::GoogleDrive,
        "nextcloud" => ExportKind::Nextcloud,
        "dropbox" => ExportKind::Dropbox,
        _ => ExportKind::S3,
    }
}

pub fn rule_name(r: AutoUpload) -> &'static str {
    match r {
        AutoUpload::Off => "off",
        AutoUpload::Protected => "protected",
        AutoUpload::Person => "person",
        AutoUpload::Motion => "motion",
        AutoUpload::AllEvents => "all_events",
    }
}

fn parse_rule(s: &str) -> AutoUpload {
    match s {
        "protected" => AutoUpload::Protected,
        "person" => AutoUpload::Person,
        "motion" => AutoUpload::Motion,
        "all_events" => AutoUpload::AllEvents,
        _ => AutoUpload::Off,
    }
}

/// A destination with everything needed to use it (secret still sealed).
pub struct StoredTarget {
    pub id: String,
    pub name: String,
    pub kind: ExportKind,
    pub endpoint: String,
    pub location: String,
    pub username: String,
    pub secret_enc: Option<Vec<u8>>,
    pub auto_upload: AutoUpload,
    pub problem: Option<String>,
}

impl StoredTarget {
    /// What the browser may see: no endpoint credentials, no secret.
    pub fn public(&self) -> ExportTarget {
        ExportTarget {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            location: self.location.clone(),
            ready: self.problem.is_none(),
            problem: self.problem.clone(),
            auto_upload: self.auto_upload,
        }
    }

    /// What the edit form needs: everything but the secret.
    pub fn settings(&self) -> ExportTargetSettings {
        ExportTargetSettings {
            name: self.name.clone(),
            kind: self.kind,
            endpoint: self.endpoint.clone(),
            location: self.location.clone(),
            username: self.username.clone(),
            has_secret: self.secret_enc.is_some(),
            auto_upload: self.auto_upload,
        }
    }
}

type TargetRow = (String, String, String, String, String, String, Option<Vec<u8>>, String, Option<String>);

fn target(r: TargetRow) -> StoredTarget {
    StoredTarget { id: r.0, name: r.1, kind: parse_kind(&r.2), endpoint: r.3, location: r.4, username: r.5, secret_enc: r.6, auto_upload: parse_rule(&r.7), problem: r.8 }
}

const TARGET_COLUMNS: &str = "id, name, kind, endpoint, location, username, secret_enc, auto_upload, problem";

pub async fn targets(db: &PgPool) -> sqlx::Result<Vec<StoredTarget>> {
    let rows: Vec<TargetRow> = sqlx::query_as(&format!("SELECT {TARGET_COLUMNS} FROM export_targets ORDER BY created_at")).fetch_all(db).await?;
    Ok(rows.into_iter().map(target).collect())
}

pub async fn target_by_id(db: &PgPool, id: &str) -> sqlx::Result<Option<StoredTarget>> {
    let row: Option<TargetRow> = sqlx::query_as(&format!("SELECT {TARGET_COLUMNS} FROM export_targets WHERE id = $1")).bind(id).fetch_optional(db).await?;
    Ok(row.map(target))
}

/// Reserve an id (needed before sealing the secret, which is bound to it).
pub async fn new_target_id(db: &PgPool) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT 'exp-' || nextval('export_targets_seq')").fetch_one(db).await
}

pub async fn insert_target(db: &PgPool, t: &StoredTarget) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO export_targets (id, name, kind, endpoint, location, username, secret_enc, auto_upload) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)")
        .bind(&t.id)
        .bind(&t.name)
        .bind(kind_name(t.kind))
        .bind(&t.endpoint)
        .bind(&t.location)
        .bind(&t.username)
        .bind(&t.secret_enc)
        .bind(rule_name(t.auto_upload))
        .execute(db)
        .await
        .map(|_| ())
}

/// Save an edited destination (kind and id stay); clears its problem.
pub async fn update_target(db: &PgPool, t: &StoredTarget) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE export_targets SET name = $2, endpoint = $3, location = $4, username = $5, secret_enc = $6, auto_upload = $7, problem = NULL WHERE id = $1")
        .bind(&t.id)
        .bind(&t.name)
        .bind(&t.endpoint)
        .bind(&t.location)
        .bind(&t.username)
        .bind(&t.secret_enc)
        .bind(rule_name(t.auto_upload))
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn set_auto(db: &PgPool, id: &str, rule: AutoUpload) -> sqlx::Result<bool> {
    Ok(sqlx::query("UPDATE export_targets SET auto_upload = $2 WHERE id = $1").bind(id).bind(rule_name(rule)).execute(db).await?.rows_affected() > 0)
}

pub async fn set_problem(db: &PgPool, id: &str, problem: Option<&str>) -> sqlx::Result<()> {
    sqlx::query("UPDATE export_targets SET problem = $2 WHERE id = $1").bind(id).bind(problem).execute(db).await.map(|_| ())
}

pub async fn delete_target(db: &PgPool, id: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query("DELETE FROM export_targets WHERE id = $1").bind(id).execute(db).await?.rows_affected() > 0)
}

fn parse_state(s: &str) -> ExportState {
    match s {
        "uploading" => ExportState::Uploading,
        "done" => ExportState::Done,
        "failed" => ExportState::Failed,
        _ => ExportState::Queued,
    }
}

type JobRow = (String, Option<String>, String, String, i64, i64, Option<String>, Option<String>, DateTime<Utc>);

fn job(r: JobRow) -> ExportJob {
    let progress = if r.4 > 0 { (r.5 as f32 / r.4 as f32 * 100.0).min(100.0) } else { 0.0 };
    let state = parse_state(&r.3);
    ExportJob {
        id: r.0,
        event_id: r.1,
        target_id: r.2,
        progress: if state == ExportState::Done { 100.0 } else { progress },
        state,
        // Where the file landed stays on the server (database, logs): the UI
        // doesn't show paths inside a destination.
        link: None,
        message: r.7,
        created_at: r.8,
        camera_id: None,
        clip_start: None,
    }
}

/// Uploads not finished yet (queued, waiting for a retry, running), oldest
/// first, with their clip's camera and start.
pub async fn active_jobs(db: &PgPool) -> sqlx::Result<Vec<ExportJob>> {
    type Active = (String, Option<String>, String, String, i64, i64, Option<String>, Option<String>, DateTime<Utc>, Option<String>, Option<DateTime<Utc>>);
    let rows: Vec<Active> = sqlx::query_as(&format!(
        "SELECT {JOB_COLUMNS},
                (SELECT r.camera_id FROM recordings r WHERE r.id = export_jobs.recording_id),
                (SELECT r.start_time FROM recordings r WHERE r.id = export_jobs.recording_id)
         FROM export_jobs WHERE state IN ('queued', 'uploading') ORDER BY created_at, id"
    ))
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ExportJob { camera_id: r.9, clip_start: r.10, ..job((r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8)) })
        .collect())
}

/// Cancel a job that hasn't started (queued, or waiting for a retry).
pub async fn cancel_queued(db: &PgPool, id: &str) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE export_jobs SET state = 'failed', message = 'Cancelled', retry_at = NULL, updated_at = now() WHERE id = $1 AND state = 'queued'")
        .bind(id)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

const JOB_COLUMNS: &str = "id, event_id, target_id, state, bytes_total, bytes_done, link, message, created_at";

pub async fn job_by_id(db: &PgPool, id: &str) -> sqlx::Result<Option<ExportJob>> {
    let row: Option<JobRow> = sqlx::query_as(&format!("SELECT {JOB_COLUMNS} FROM export_jobs WHERE id = $1")).bind(id).fetch_optional(db).await?;
    Ok(row.map(job))
}

/// An unfinished or successful job for this clip and destination, if any.
pub async fn existing_job(db: &PgPool, recording_id: &str, target_id: &str) -> sqlx::Result<Option<ExportJob>> {
    let row: Option<JobRow> = sqlx::query_as(&format!(
        "SELECT {JOB_COLUMNS} FROM export_jobs WHERE recording_id = $1 AND target_id = $2 AND state <> 'failed' ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(recording_id)
    .bind(target_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(job))
}

/// Queue a job; `None` if a live one for this clip and destination already
/// exists (two enqueues at once can't make duplicates: unique index).
pub async fn insert_job(db: impl sqlx::PgExecutor<'_>, event_id: Option<&str>, recording_id: &str, target_id: &str) -> sqlx::Result<Option<ExportJob>> {
    let row: Option<JobRow> = sqlx::query_as(&format!(
        "INSERT INTO export_jobs (event_id, recording_id, target_id, state) VALUES ($1, $2, $3, 'queued')
         ON CONFLICT (recording_id, target_id) WHERE state <> 'failed' DO NOTHING RETURNING {JOB_COLUMNS}"
    ))
    .bind(event_id)
    .bind(recording_id)
    .bind(target_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(job))
}

/// (event, recording, target) of a job.
pub async fn job_parts(db: &PgPool, id: &str) -> sqlx::Result<Option<(Option<String>, String, String)>> {
    sqlx::query_as("SELECT event_id, recording_id, target_id FROM export_jobs WHERE id = $1").bind(id).fetch_optional(db).await
}

/// `false`: the job isn't queued any more (cancelled meanwhile).
pub async fn job_started(db: &PgPool, id: &str, total: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE export_jobs SET state = 'uploading', bytes_total = $2, bytes_done = 0, retry_at = NULL, message = NULL, updated_at = now() WHERE id = $1 AND state = 'queued'")
        .bind(id)
        .bind(total)
        .execute(db)
        .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn job_progress(db: &PgPool, id: &str, done: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE export_jobs SET bytes_done = $2, updated_at = now() WHERE id = $1").bind(id).bind(done).execute(db).await.map(|_| ())
}

pub async fn job_finished(db: &PgPool, id: &str, result: &Result<String, String>) -> sqlx::Result<()> {
    let (state, link, message) = match result {
        Ok(link) => ("done", Some(link.as_str()), None),
        Err(e) => ("failed", None, Some(e.as_str())),
    };
    sqlx::query("UPDATE export_jobs SET state = $2, link = $3, message = $4, bytes_done = CASE WHEN $2 = 'done' THEN bytes_total ELSE bytes_done END, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(state)
        .bind(link)
        .bind(message)
        .execute(db)
        .await
        .map(|_| ())
}

/// Retries so far.
pub async fn job_attempts(db: &PgPool, id: &str) -> sqlx::Result<i32> {
    Ok(sqlx::query_scalar("SELECT attempts FROM export_jobs WHERE id = $1").bind(id).fetch_optional(db).await?.unwrap_or(0))
}

/// Back in the queue, due at `at`; the error stays visible meanwhile.
pub async fn job_retry_later(db: &PgPool, id: &str, message: &str, at: DateTime<Utc>) -> sqlx::Result<()> {
    sqlx::query("UPDATE export_jobs SET state = 'queued', bytes_done = 0, attempts = attempts + 1, message = $2, retry_at = $3, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(message)
        .bind(at)
        .execute(db)
        .await
        .map(|_| ())
}

/// A waiting retry made due now (someone asked for the upload again).
/// Due retries drop the last error: a queued job with a message is waiting.
pub async fn retry_now(db: &PgPool, id: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query("UPDATE export_jobs SET retry_at = NULL, message = NULL WHERE id = $1 AND state = 'queued' AND retry_at IS NOT NULL").bind(id).execute(db).await?.rows_affected() > 0)
}

/// Retries whose time has come, handed to the queue.
pub async fn take_due_retries(db: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("UPDATE export_jobs SET retry_at = NULL, message = NULL WHERE state = 'queued' AND retry_at <= now() RETURNING id").fetch_all(db).await
}

/// Jobs interrupted by a restart go back in the queue (waiting retries
/// keep their time).
pub async fn requeue_unfinished(db: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("UPDATE export_jobs SET state = 'queued', bytes_done = 0 WHERE state = 'uploading' OR (state = 'queued' AND retry_at IS NULL) RETURNING id")
        .fetch_all(db)
        .await
}
