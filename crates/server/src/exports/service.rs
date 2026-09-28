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
/// A job that hasn't started this long after it was queued is skipped: the
/// uplink can't keep up, and a queue that never empties would keep
/// retention from deleting old clips (the disk would fill).
const MAX_WAIT: Duration = Duration::from_secs(24 * 3600);
const SKIPPED: &str = "Skipped: waited more than 24 h — the uplink can't keep up with the recordings";

/// Waits before each retry of a failed upload; then it stays failed.
const RETRY_AFTER: [Duration; 4] = [Duration::from_secs(60), Duration::from_secs(5 * 60), Duration::from_secs(15 * 60), Duration::from_secs(60 * 60)];

/// Why an upload failed: `Final` when trying again can't help.
enum Failure {
    Final(String),
    Transient(String),
    /// Stopped on request while running.
    Cancelled,
    /// Not started: cancelled while it was queued (already recorded).
    Skipped,
}

impl Failure {
    /// A provider's error: the server refused the request (4xx: credentials,
    /// bucket, path) or something that may pass later (network, 5xx, stall).
    fn of(message: String) -> Self {
        let status = message.split("HTTP ").nth(1).and_then(|s| s.get(..3)).and_then(|s| s.parse::<u16>().ok());
        match status {
            Some(s) if (400..500).contains(&s) && s != 408 && s != 429 => Self::Final(message),
            _ => Self::Transient(message),
        }
    }
}

pub struct Exports {
    db: PgPool,
    credentials: Arc<CredentialStore>,
    files: Arc<RecordingFiles>,
    queue: mpsc::UnboundedSender<String>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    /// Running uploads, each with its stop signal.
    running: std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Notify>>>,
}

pub fn secret_aad(target_id: &str) -> String {
    format!("export:{target_id}:secret")
}

impl Exports {
    pub fn new(db: PgPool, credentials: Arc<CredentialStore>, files: Arc<RecordingFiles>) -> Self {
        let (queue, rx) = mpsc::unbounded_channel();
        Self { db, credentials, files, queue, receiver: Mutex::new(Some(rx)), running: Default::default() }
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
        let me = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                match repo::skip_stale(&me.db, MAX_WAIT, SKIPPED).await {
                    Ok(0) => {}
                    Ok(n) => tracing::warn!(skipped = n, "export uploads skipped after waiting more than 24 h: the uplink can't keep up"),
                    Err(e) => tracing::warn!("cannot check the export backlog: {e}"),
                }
                match repo::take_due_retries(&me.db).await {
                    Ok(ids) => ids.into_iter().for_each(|id| drop(me.queue.send(id))),
                    Err(e) => tracing::warn!("cannot check export retries: {e}"),
                }
            }
        });
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
        // What matters is the clip being saved, not the event having ended.
        if crate::recordings::get(&self.db, &recording_id).await?.is_none() {
            return Err(ApiError::conflict("The clip is still being recorded; it is uploaded once saved if a rule asks for it"));
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
            // Waiting for a retry: asked again, so try now.
            if repo::retry_now(&self.db, &job.id).await? {
                let _ = self.queue.send(job.id.clone());
            }
            return Ok(job);
        }
        // Under the clip's lock: a delete either ran before (no clip, no job)
        // or sees this job and waits for it.
        let mut tx = self.db.begin().await?;
        crate::recordings::lock_clip(&mut tx, recording_id).await?;
        if !crate::recordings::clip_exists(&mut tx, recording_id).await? {
            return Err(ApiError::conflict("The clip was deleted"));
        }
        let inserted = repo::insert_job(&mut *tx, event_id, recording_id, target_id).await?;
        tx.commit().await?;
        let Some(job) = inserted else {
            // Queued by someone else just now.
            return repo::existing_job(&self.db, recording_id, target_id).await?.ok_or_else(|| ApiError::conflict("The export could not be queued; try again"));
        };
        let _ = self.queue.send(job.id.clone());
        tracing::info!(job = %job.id, recording = %recording_id, target = %target_id, "export queued");
        Ok(job)
    }

    /// Auto-upload after a clip was saved, per destination rule —
    /// including "Protected" for a clip protected while it was recording.
    pub async fn on_clip_saved(&self, recording_id: &str) {
        let Ok(targets) = repo::targets(&self.db).await else { return };
        let wanted: Vec<_> = targets.into_iter().filter(|t| t.auto_upload != AutoUpload::Off && t.problem.is_none()).collect();
        if wanted.is_empty() {
            return;
        }
        // The journal links the clip's detections when it is saved; do it
        // here too (idempotent) so the rules never see a clip before that.
        let _ = crate::events::link_detections_within(&self.db, recording_id).await;
        let events = crate::events::events_of_recording(&self.db, recording_id).await.unwrap_or_default();
        let protected = crate::recordings::get(&self.db, recording_id).await.ok().flatten().is_some_and(|r| r.is_protected());
        for t in wanted {
            let result = match t.auto_upload {
                AutoUpload::Protected if protected => {
                    let event = events.iter().find(|e| e.protected).map(|e| e.id.as_str());
                    self.queue_job(event, recording_id, &t.id).await.map(|_| ())
                }
                rule => match pick(rule, &events) {
                    Some(e) => self.enqueue(&e.id, &t.id).await.map(|_| ()),
                    None => Ok(()),
                },
            };
            if let Err(err) = result {
                tracing::warn!(target = %t.id, recording = %recording_id, "auto-upload skipped: {}", err.message());
            }
        }
    }

    /// Auto-upload when an event is protected, per destination rule. A clip
    /// still recording is handled when it is saved (`on_clip_saved`).
    pub async fn on_protected(&self, event_id: &str) {
        let Ok(Some(event)) = crate::events::get_event(&self.db, event_id).await else { return };
        let Some(recording_id) = event.recording_id else { return };
        if matches!(crate::recordings::get(&self.db, &recording_id).await, Ok(Some(_))) {
            self.on_recording_protected(&recording_id, Some(event_id)).await;
        }
    }

    /// Auto-upload when a saved clip becomes protected (by hand or by an event).
    pub async fn on_recording_protected(&self, recording_id: &str, event_id: Option<&str>) {
        let Ok(targets) = repo::targets(&self.db).await else { return };
        for t in targets.into_iter().filter(|t| t.auto_upload == AutoUpload::Protected && t.problem.is_none()) {
            if let Err(err) = self.queue_job(event_id, recording_id, &t.id).await {
                tracing::warn!(target = %t.id, recording = %recording_id, "auto-upload skipped: {}", err.message());
            }
        }
    }

    /// Stop an upload: a queued one (or one waiting for a retry) is marked
    /// cancelled, a running one is stopped. Finished ones are left alone.
    pub async fn cancel(&self, id: &str) -> ApiResult<ExportJob> {
        if !repo::cancel_queued(&self.db, id).await? {
            let signal = self.running.lock().expect("running uploads lock").get(id).cloned();
            match signal {
                Some(stop) => stop.notify_one(),
                None => return Err(ApiError::conflict("This upload has already finished")),
            }
        }
        tracing::info!(job = %id, "export cancelled");
        repo::job_by_id(&self.db, id).await?.ok_or_else(|| ApiError::not_found("Export"))
    }

    async fn run_job(&self, id: &str) {
        let result = match self.upload(id).await {
            Err(Failure::Skipped) => return,
            Err(Failure::Cancelled) => Err("Cancelled".to_string()),
            Ok(link) => {
                tracing::info!(job = %id, "export done");
                Ok(link)
            }
            Err(Failure::Transient(e)) => {
                let retries = repo::job_attempts(&self.db, id).await.unwrap_or(0) as usize;
                match RETRY_AFTER.get(retries) {
                    Some(wait) => {
                        tracing::warn!(job = %id, "export failed, retrying in {} min: {e}", wait.as_secs() / 60);
                        let at = chrono::Utc::now() + chrono::Duration::from_std(*wait).unwrap_or_default();
                        if let Err(err) = repo::job_retry_later(&self.db, id, &e, at).await {
                            tracing::warn!(job = %id, "cannot schedule the export retry: {err}");
                        }
                        return;
                    }
                    None => {
                        tracing::warn!(job = %id, "export failed, giving up: {e}");
                        Err(format!("{e} (gave up after {} tries)", retries + 1))
                    }
                }
            }
            Err(Failure::Final(e)) => {
                tracing::warn!(job = %id, "export failed: {e}");
                Err(e)
            }
        };
        if let Err(e) = repo::job_finished(&self.db, id, &result).await {
            tracing::warn!(job = %id, "cannot store export result: {e}");
        }
    }

    async fn upload(&self, id: &str) -> Result<String, Failure> {
        // A database hiccup may pass; a missing clip or destination won't.
        let db = |e: sqlx::Error| Failure::Transient(e.to_string());
        let gone = |what: &str| Failure::Final(what.to_string());
        let (event_id, recording_id, target_id) = repo::job_parts(&self.db, id).await.map_err(db)?.ok_or_else(|| gone("job vanished"))?;
        let target = repo::target_by_id(&self.db, &target_id).await.map_err(db)?.ok_or_else(|| gone("the destination was deleted"))?;
        let provider = self.provider(&target).map_err(Failure::Final)?;
        let recording = crate::recordings::get(&self.db, &recording_id).await.map_err(|e| Failure::Transient(e.to_string()))?.ok_or_else(|| gone("the clip was deleted"))?;
        let file = crate::recordings::file_of(&self.db, &self.files, &recording_id)
            .await
            .map_err(|e| Failure::Transient(e.to_string()))?
            .ok_or_else(|| gone("the clip file is missing"))?;
        let size = tokio::fs::metadata(&file).await.map_err(|e| Failure::Final(format!("the clip file is missing: {e}")))?.len();
        if !repo::job_started(&self.db, id, size as i64).await.map_err(db)? {
            return Err(Failure::Skipped);
        }

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
        let upload = provider.upload(&file, &name, move |n| counter.store(n, Ordering::Relaxed));
        let stop = Arc::new(tokio::sync::Notify::new());
        self.running.lock().expect("running uploads lock").insert(id.to_string(), stop.clone());
        let result = tokio::select! {
            r = watch_progress(upload, &sent) => Some(r),
            () = stop.notified() => None,
        };
        self.running.lock().expect("running uploads lock").remove(id);
        reporter.abort();
        let Some(result) = result else { return Err(Failure::Cancelled) };
        // Credentials rejected: mark the destination so the UI says so.
        if let Err(e) = &result
            && (e.contains("HTTP 401") || e.contains("HTTP 403"))
        {
            let _ = repo::set_problem(&self.db, &target.id, Some(e)).await;
        }
        result.map_err(Failure::of)
    }
}

/// An upload that sends nothing for this long is given up (large healthy
/// uploads keep moving, so they are never cut short).
const STALL: Duration = Duration::from_secs(120);

/// Run `upload`, failing it when `sent` stops growing for `STALL`.
async fn watch_progress(upload: impl std::future::Future<Output = Result<String, String>>, sent: &AtomicU64) -> Result<String, String> {
    tokio::pin!(upload);
    let mut last = (sent.load(Ordering::Relaxed), tokio::time::Instant::now());
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            result = &mut upload => return result,
            _ = tick.tick() => {
                let now = sent.load(Ordering::Relaxed);
                if now != last.0 {
                    last = (now, tokio::time::Instant::now());
                } else if last.1.elapsed() >= STALL {
                    return Err(format!("the upload stalled (nothing sent for {} s)", STALL.as_secs()));
                }
            }
        }
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

    #[tokio::test(start_paused = true)]
    async fn a_stalled_upload_is_given_up_and_a_moving_one_is_not() {
        let sent = AtomicU64::new(0);
        let stuck = std::future::pending::<Result<String, String>>();
        let err = watch_progress(stuck, &sent).await.unwrap_err();
        assert!(err.contains("stalled"), "{err}");

        // Progress every 10 s for 5 minutes: slow but alive.
        let sent = Arc::new(AtomicU64::new(0));
        let feeder = sent.clone();
        let slow = async move {
            for i in 1..=30 {
                tokio::time::sleep(Duration::from_secs(10)).await;
                feeder.store(i, Ordering::Relaxed);
            }
            Ok("done".to_string())
        };
        assert_eq!(watch_progress(slow, &sent).await.unwrap(), "done");
    }

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
    fn refusals_are_final_and_outages_are_retried() {
        let final_ = |m: &str| matches!(Failure::of(m.into()), Failure::Final(_));
        assert!(final_("upload failed: HTTP 403 Forbidden: bad signature — check the credentials and permissions"));
        assert!(final_("upload failed: HTTP 404 Not Found — check the bucket / folder"));
        assert!(!final_("upload failed: HTTP 503 Service Unavailable"));
        assert!(!final_("upload failed: HTTP 429 Too Many Requests"));
        assert!(!final_("upload failed: error sending request: connection refused"));
        assert!(!final_("the upload stalled (nothing sent for 120 s)"));
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
