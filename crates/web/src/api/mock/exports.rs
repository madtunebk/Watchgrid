//! Mock exports: jobs "upload" over a few seconds based on elapsed time.

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use gloo_timers::future::TimeoutFuture;

use crate::api::{ApiError, ApiResult, AutoUpload, ConnectionProbe, ExportJob, ExportKind, ExportState, ExportTarget, ExportTargetInput, ExportTargetSettings};

/// Simulated upload time.
const UPLOAD_MS: i64 = 4_000;

pub async fn targets() -> ApiResult<Vec<ExportTarget>> {
    latency().await;
    Ok(with_db(|db| db.export_targets.clone()))
}

pub async fn start(event_id: &str, target_id: &str) -> ApiResult<ExportJob> {
    latency().await;
    if !with_db(|db| db.events.iter().any(|e| e.id == event_id)) {
        return Err(ApiError::not_found("Event"));
    }
    queue(Some(event_id), target_id)
}

pub async fn start_recording(recording_id: &str, target_id: &str) -> ApiResult<ExportJob> {
    latency().await;
    if !with_db(|db| db.recordings.iter().any(|r| r.id == recording_id)) {
        return Err(ApiError::not_found("Recording"));
    }
    queue(None, target_id)
}

fn queue(event_id: Option<&str>, target_id: &str) -> ApiResult<ExportJob> {
    with_db(|db| {
        let target = db.export_targets.iter().find(|t| t.id == target_id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        if !target.ready {
            return Err(ApiError::conflict(target.problem.clone().unwrap_or_else(|| "Destination not ready".into())));
        }
        let job = ExportJob {
            id: format!("exp-{}", db.export_jobs.len() + 1),
            event_id: event_id.map(String::from),
            target_id: target_id.into(),
            state: ExportState::Queued,
            progress: 0.0,
            link: None,
            message: None,
            created_at: Utc::now(),
            camera_id: None,
            clip_start: None,
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
        if job.state == ExportState::Failed {
            return Ok(job.clone()); // cancelled
        }
        (job.state, job.progress) = match elapsed {
            e if e < 500 => (ExportState::Queued, 0.0),
            e if e < UPLOAD_MS => (ExportState::Uploading, (e as f32 / UPLOAD_MS as f32 * 100.0).min(99.0)),
            _ => (ExportState::Done, 100.0),
        };
        if job.state == ExportState::Done && job.link.is_none() {
        }
        Ok(job.clone())
    })
}

/// Demo uploads finish within seconds: the ones still running.
pub async fn pending() -> ApiResult<Vec<ExportJob>> {
    latency().await;
    Ok(with_db(|db| {
        db.export_jobs.iter().filter(|j| (Utc::now() - j.created_at).num_milliseconds() < UPLOAD_MS && j.message.is_none()).cloned().collect()
    }))
}

pub async fn cancel(id: &str) -> ApiResult<ExportJob> {
    latency().await;
    with_db(|db| {
        let job = db.export_jobs.iter_mut().find(|j| j.id == id).ok_or_else(|| ApiError::not_found("Export"))?;
        if job.state == ExportState::Done {
            return Err(ApiError::conflict("This upload has already finished"));
        }
        job.state = ExportState::Failed;
        job.message = Some("Cancelled".into());
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

/// The demo keeps no endpoints or keys: the form shows them empty.
pub async fn settings(id: &str) -> ApiResult<ExportTargetSettings> {
    latency().await;
    with_db(|db| {
        let t = db.export_targets.iter().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        Ok(ExportTargetSettings {
            name: t.name.clone(),
            kind: t.kind,
            endpoint: String::new(),
            location: t.location.clone(),
            username: String::new(),
            has_secret: true,
            auto_upload: t.auto_upload,
        })
    })
}

pub async fn test_saved(_id: &str, input: &ExportTargetInput) -> ApiResult<ConnectionProbe> {
    let input = ExportTargetInput { secret: Some("saved".into()), ..input.clone() };
    test(&input).await
}

pub async fn update(id: &str, input: ExportTargetInput) -> ApiResult<ExportTarget> {
    latency().await;
    if input.name.trim().is_empty() {
        return Err(ApiError::new(422, "invalid", "Give the destination a name"));
    }
    with_db(|db| {
        let t = db.export_targets.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        t.name = input.name.trim().into();
        t.location = input.location;
        t.auto_upload = input.auto_upload;
        t.ready = true;
        t.problem = None;
        Ok(t.clone())
    })
}

pub async fn set_auto(id: &str, rule: AutoUpload) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let t = db.export_targets.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        t.auto_upload = rule;
        Ok(())
    })
}

pub async fn check(id: &str) -> ApiResult<ConnectionProbe> {
    TimeoutFuture::new(400).await;
    with_db(|db| {
        let t = db.export_targets.iter().find(|t| t.id == id).ok_or_else(|| ApiError::not_found("Export destination"))?;
        Ok(match &t.problem {
            None => ConnectionProbe { ok: true, message: "Connected".into(), latency_ms: Some(38), device: None },
            Some(p) => ConnectionProbe { ok: false, message: p.clone(), latency_ms: None, device: None },
        })
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
