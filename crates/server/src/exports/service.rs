//! The export queue: jobs are uploaded by a small pool of workers, one
//! clip at a time each, with progress written to the database.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::{Mutex, mpsc};
use watchgrid_model::{AutoUpload, Event, EventType, ExportJob};

use super::providers::{Provider, TargetConfig};
use super::repo::{self, StoredTarget};
use crate::credentials::CredentialStore;
use crate::error::{ApiError, ApiResult};
use crate::recordings::RecordingFiles;

/// Uploads running at once (they compete for the uplink).
const WORKERS: usize = 2;

pub struct Exports {
    db: PgPool,
    credentials: Arc<CredentialStore>,
    files: Arc<RecordingFiles>,
    queue: mpsc::UnboundedSender<String>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

pub fn secret_aad(target_id: &str) -> String {
    format!("export:{target_id}:secret")
}

impl Exports {
    pub fn new(db: PgPool, credentials: Arc<CredentialStore>, files: Arc<RecordingFiles>) -> Self {
        let (queue, rx) = mpsc::unbounded_channel();
        Self { db, credentials, files, queue, receiver: Mutex::new(Some(rx)) }
    }

    /// Start the workers and put back jobs interrupted by a restart.
    pub async fn start(self: &Arc<Self>) {
        let Some(rx) = self.receiver.lock().await.take() else { return };
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..WORKERS {
            let (me, rx) = (self.clone(), rx.clone());
            tokio::spawn(async move {
                loop {
                    let next = rx.lock().await.recv().await;
                    let Some(id) = next else { return };
                    me.run_job(&id).await;
                }
            });
        }
        match repo::requeue_unfinished(&self.db).await {
            Ok(ids) => {
                if !ids.is_empty() {
                    tracing::info!("resuming {} export job(s)", ids.len());
                }
                for id in ids {
                    let _ = self.queue.send(id);
                }
            }
            Err(e) => tracing::warn!("cannot resume export jobs: {e}"),
        }
    }

    /// Decrypted provider for a stored destination.
    pub fn provider(&self, t: &StoredTarget) -> Result<Provider, String> {
        let secret = match &t.secret_enc {
            Some(enc) => String::from_utf8(self.credentials.open(&secret_aad(&t.id), enc).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
            None => String::new(),
        };
        Provider::new(&TargetConfig { kind: t.kind, endpoint: &t.endpoint, location: &t.location, username: &t.username, secret: &secret })
    }

    /// Queue the upload of an event's clip (reuses a pending or finished job).
    pub async fn enqueue(&self, event_id: &str, target_id: &str) -> ApiResult<ExportJob> {
        let target = repo::target_by_id(&self.db, target_id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
        if let Some(p) = &target.problem {
            return Err(ApiError::conflict(format!("{} isn't ready: {p}", target.name)));
        }
        let event = crate::events::get_event(&self.db, event_id).await?.ok_or_else(|| ApiError::not_found("Event"))?;
        let recording_id = event.recording_id.ok_or_else(|| ApiError::conflict("This event has no video clip"))?;
        if event.end_time.is_none() {
            return Err(ApiError::conflict("The clip is still being recorded; try again when it has finished"));
        }
        self.queue_job(Some(event_id), &recording_id, target_id).await
    }

    /// Upload a saved recording directly (e.g. a continuous clip).
    pub async fn enqueue_recording(&self, recording_id: &str, target_id: &str) -> ApiResult<ExportJob> {
        let target = repo::target_by_id(&self.db, target_id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
        if let Some(p) = &target.problem {
            return Err(ApiError::conflict(format!("{} isn't ready: {p}", target.name)));
        }
        // Rows exist only for finished, saved clips.
        crate::recordings::get(&self.db, recording_id).await?.ok_or_else(|| ApiError::not_found("Recording"))?;
        self.queue_job(None, recording_id, target_id).await
    }

    async fn queue_job(&self, event_id: Option<&str>, recording_id: &str, target_id: &str) -> ApiResult<ExportJob> {
        if let Some(job) = repo::existing_job(&self.db, recording_id, target_id).await? {
            return Ok(job);
        }
        let job = repo::insert_job(&self.db, event_id, recording_id, target_id).await?;
        let _ = self.queue.send(job.id.clone());
        tracing::info!(job = %job.id, recording = %recording_id, target = %target_id, "export queued");
        Ok(job)
    }

    /// Auto-upload after a clip was saved, per destination rule.
    pub async fn on_clip_saved(&self, recording_id: &str) {
        let Ok(targets) = repo::targets(&self.db).await else { return };
        let wanted: Vec<_> = targets.into_iter().filter(|t| matches!(t.auto_upload, AutoUpload::AllEvents | AutoUpload::Person | AutoUpload::Motion) && t.problem.is_none()).collect();
        if wanted.is_empty() {
            return;
        }
        let events = crate::events::events_of_recording(&self.db, recording_id).await.unwrap_or_default();
        for t in wanted {
            if let Some(e) = pick(t.auto_upload, &events)
                && let Err(err) = self.enqueue(&e.id, &t.id).await
            {
                tracing::warn!(target = %t.id, event = %e.id, "auto-upload skipped: {}", err.message());
            }
        }
    }

    /// Auto-upload when an event is protected, per destination rule.
    pub async fn on_protected(&self, event_id: &str) {
        let Ok(targets) = repo::targets(&self.db).await else { return };
        for t in targets.into_iter().filter(|t| t.auto_upload == AutoUpload::Protected && t.problem.is_none()) {
            if let Err(err) = self.enqueue(event_id, &t.id).await {
                tracing::warn!(target = %t.id, event = %event_id, "auto-upload skipped: {}", err.message());
            }
        }
    }

    async fn run_job(&self, id: &str) {
        let result = self.upload(id).await;
        if let Err(e) = &result {
            tracing::warn!(job = %id, "export failed: {e}");
        } else {
            tracing::info!(job = %id, "export done");
        }
        if let Err(e) = repo::job_finished(&self.db, id, &result).await {
            tracing::warn!(job = %id, "cannot store export result: {e}");
        }
    }

    async fn upload(&self, id: &str) -> Result<String, String> {
        let (event_id, recording_id, target_id) = repo::job_parts(&self.db, id).await.map_err(|e| e.to_string())?.ok_or("job vanished")?;
        let target = repo::target_by_id(&self.db, &target_id).await.map_err(|e| e.to_string())?.ok_or("the destination was deleted")?;
        let provider = self.provider(&target)?;
        let recording = crate::recordings::get(&self.db, &recording_id).await.map_err(|e| e.to_string())?.ok_or("the clip was deleted")?;
        let file = crate::recordings::file_of(&self.db, &self.files, &recording_id).await.map_err(|e| e.to_string())?.ok_or("the clip file is missing")?;
        let size = tokio::fs::metadata(&file).await.map_err(|e| format!("the clip file is missing: {e}"))?.len();
        repo::job_started(&self.db, id, size as i64).await.map_err(|e| e.to_string())?;

        // `<camera>/<day>/<camera>_<time>Z[_<event>].mp4` (UTC).
        let t = recording.start_time;
        let event = event_id.map(|e| format!("_{e}")).unwrap_or_default();
        let name = format!("{c}/{}/{c}_{}Z{event}.mp4", t.format("%Y-%m-%d"), t.format("%Y%m%d-%H%M%S"), c = recording.camera_id);

        let sent = Arc::new(AtomicU64::new(0));
        let reporter = {
            let (db, id, sent) = (self.db.clone(), id.to_string(), sent.clone());
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let _ = repo::job_progress(&db, &id, sent.load(Ordering::Relaxed) as i64).await;
                }
            })
        };
        let counter = sent.clone();
        let result = provider.upload(&file, &name, move |n| counter.store(n, Ordering::Relaxed)).await;
        reporter.abort();
        // Credentials rejected: mark the destination so the UI says so.
        if let Err(e) = &result
            && (e.contains("HTTP 401") || e.contains("HTTP 403"))
        {
            let _ = repo::set_problem(&self.db, &target.id, Some(e)).await;
        }
        result
    }
}

/// The clip's event a rule uploads under, if the rule wants the clip.
fn pick(rule: AutoUpload, events: &[Event]) -> Option<&Event> {
    match rule {
        AutoUpload::Person => events.iter().find(|e| e.kind == EventType::Person),
        AutoUpload::Motion => events.iter().find(|e| matches!(e.kind, EventType::Motion | EventType::Person | EventType::Vehicle | EventType::Animal)),
        AutoUpload::AllEvents => events.first(),
        AutoUpload::Off | AutoUpload::Protected => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, kind: EventType) -> Event {
        let at = chrono::Utc::now();
        Event {
            id: id.into(),
            camera_id: "cam".into(),
            kind,
            start_time: at,
            end_time: Some(at),
            duration: 0,
            recording_id: Some("rec".into()),
            thumbnail: None,
            protected: false,
            detections: Vec::new(),
            source: String::new(),
        }
    }

    #[test]
    fn rules_pick_the_matching_event() {
        let clip = [event("e1", EventType::Scheduled), event("e2", EventType::Motion), event("e3", EventType::Person)];
        assert_eq!(pick(AutoUpload::Motion, &clip).map(|e| e.id.as_str()), Some("e2"));
        assert_eq!(pick(AutoUpload::Person, &clip).map(|e| e.id.as_str()), Some("e3"));
        assert_eq!(pick(AutoUpload::AllEvents, &clip).map(|e| e.id.as_str()), Some("e1"));
        assert_eq!(pick(AutoUpload::Motion, &clip[..1]), None, "no detection in the clip");
        assert_eq!(pick(AutoUpload::Protected, &clip), None, "uploaded when protected instead");
    }
}
