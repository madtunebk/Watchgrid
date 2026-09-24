//! Mock exports: jobs "upload" over a few seconds based on elapsed time.

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use gloo_timers::future::TimeoutFuture;

use crate::api::{ApiError, ApiResult, AutoUpload, ConnectionProbe, ExportJob, ExportKind, ExportState, ExportTarget, ExportTargetInput};

/// Simulated upload time.
const UPLOAD_MS: i64 = 4_000;

pub async fn targets() -> ApiResult<Vec<ExportTarget>> {
    latency().await;
    Ok(with_db(|db| db.export_targets.clone()))
}

pub async fn start(event_id: &str, target_id: &str) -> ApiResult<ExportJob> {
    latency().await;
    with_db(|db| {
        let target = db.export_targets.iter().find(|t| t.id == target_id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        if !target.ready {
            return Err(ApiError::conflict(target.problem.clone().unwrap_or_else(|| "Destination not ready".into())));
        }
        if !db.events.iter().any(|e| e.id == event_id) {
            return Err(ApiError::not_found("Event"));
        }
        let job = ExportJob {
            id: format!("exp-{}", db.export_jobs.len() + 1),
            event_id: event_id.into(),
            target_id: target_id.into(),
            state: ExportState::Queued,
            progress: 0.0,
            link: None,
            message: None,
            created_at: Utc::now(),
        };
        db.export_jobs.push(job.clone());
        Ok(job)
    })
}

pub async fn job(id: &str) -> ApiResult<ExportJob> {
    latency().await;
    with_db(|db| {
        let job = db.export_jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| ApiError::not_found("Export"))?;
        let elapsed = (Utc::now() - job.created_at).num_milliseconds();
        (job.state, job.progress) = match elapsed {
            e if e < 500 => (ExportState::Queued, 0.0),
            e if e < UPLOAD_MS => (ExportState::Uploading, (e as f32 / UPLOAD_MS as f32 * 100.0).min(99.0)),
            _ => (ExportState::Done, 100.0),
        };
        if job.state == ExportState::Done && job.link.is_none() {
            job.link = (job.target_id == "gdrive").then(|| "https://drive.google.com/".to_string());
        }
        Ok(job.clone())
    })
}

pub async fn test(input: &ExportTargetInput) -> ApiResult<ConnectionProbe> {
    TimeoutFuture::new(900).await;
    let fail = |m: &str| Ok(ConnectionProbe { ok: false, message: m.into(), latency_ms: None, device: None });
    match input.kind {
        ExportKind::S3 | ExportKind::Nextcloud if !(input.endpoint.starts_with("http://") || input.endpoint.starts_with("https://")) => {
            fail("Server URL must start with http:// or https://")
        }
        ExportKind::S3 | ExportKind::Nextcloud if input.secret.as_deref().unwrap_or("").is_empty() => fail("Enter the secret / app password"),
        _ if input.location.trim().is_empty() => fail("Enter a bucket or folder"),
        _ => Ok(ConnectionProbe { ok: true, message: "Destination reachable, write test succeeded".into(), latency_ms: Some(38), device: None }),
    }
}

pub async fn create(input: ExportTargetInput) -> ApiResult<ExportTarget> {
    latency().await;
    if input.name.trim().is_empty() {
        return Err(ApiError::new(422, "invalid", "Give the destination a name"));
    }
    Ok(with_db(|db| {
        let target = ExportTarget {
            id: format!("dest-{}", db.export_targets.len() + 1),
            name: input.name.trim().into(),
            kind: input.kind,
            location: input.location,
            ready: true,
            problem: None,
            auto_upload: input.auto_upload,
        };
        db.export_targets.push(target.clone());
        target
    }))
}

pub async fn set_auto(id: &str, rule: AutoUpload) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let t = db.export_targets.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        t.auto_upload = rule;
        Ok(())
    })
}

pub async fn reconnect(id: &str) -> ApiResult<()> {
    TimeoutFuture::new(1200).await;
    with_db(|db| {
        let t = db.export_targets.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        t.ready = true;
        t.problem = None;
        Ok(())
    })
}

pub async fn delete(id: &str) -> ApiResult<()> {
    latency().await;
    with_db(|db| db.export_targets.retain(|t| t.id != id));
    Ok(())
}
