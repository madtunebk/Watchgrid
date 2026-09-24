//! Event recording: for cameras in "events" mode, record while detections
//! are active, with pre-record (buffered video before the event),
//! post-record, merging of close events and splitting of long clips.
//!
//! One controller task per camera. It keeps the camera's recording stream
//! open (so the pre-record buffer is filled) and reacts to
//! DetectionStarted/Ended on the bus. Manual recordings are never touched.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use watchgrid_model::{EventType, MotionSource, RecordingMode, RecordingReason, RecordingSettings, StreamRole};

use super::{Recorder, Spec};
use crate::bus::{Bus, BusEvent};
use crate::media::{MediaHub, StreamKind};

/// Pre-record is capped to bound memory (frames are kept in RAM).
const MAX_PREROLL_SECS: u32 = 30;
/// Reasons this controller starts, and therefore may stop.
const OWN: [RecordingReason; 2] = [RecordingReason::Motion, RecordingReason::Event];

pub struct AutoRecorders {
    db: PgPool,
    hub: Arc<MediaHub>,
    bus: Bus,
    recorder: Arc<Recorder>,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    enabled: bool,
}

impl AutoRecorders {
    pub fn new(db: PgPool, hub: Arc<MediaHub>, bus: Bus, recorder: Arc<Recorder>) -> Self {
        Self { db, hub, bus, recorder, tasks: Mutex::new(HashMap::new()), enabled: true }
    }

    #[cfg(test)]
    pub fn inert(db: PgPool, hub: Arc<MediaHub>, bus: Bus, recorder: Arc<Recorder>) -> Self {
        Self { enabled: false, ..Self::new(db, hub, bus, recorder) }
    }

    /// (Re)start after the camera was added or changed.
    pub fn apply(&self, id: &str, exists: bool) {
        if let Some(old) = self.tasks.lock().expect("auto lock").remove(id) {
            old.abort();
        }
        if exists && self.enabled {
            let ctl = Controller { db: self.db.clone(), hub: self.hub.clone(), bus: self.bus.clone(), recorder: self.recorder.clone(), id: id.to_string() };
            self.tasks.lock().expect("auto lock").insert(id.to_string(), tokio::spawn(ctl.run()));
        }
    }
}

struct Controller {
    db: PgPool,
    hub: Arc<MediaHub>,
    bus: Bus,
    recorder: Arc<Recorder>,
    id: String,
}

/// The settings that matter here, in usable units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub stream: StreamKind,
    pub preroll_secs: u32,
    /// Keep recording this long after the last detection ends.
    pub hold: Duration,
    pub max_clip: Duration,
}

impl Plan {
    pub fn from_settings(r: &RecordingSettings) -> Self {
        Self {
            stream: if r.stream == StreamRole::Sub { StreamKind::Sub } else { StreamKind::Main },
            preroll_secs: r.pre_record_seconds.min(MAX_PREROLL_SECS),
            // Post-record, but at least the merge window so close events share a clip.
            hold: Duration::from_secs(u64::from(r.post_record_seconds.max(r.event_merge_seconds))),
            max_clip: Duration::from_secs(u64::from(r.max_clip_seconds.max(30))),
        }
    }
}

impl Controller {
    async fn run(self) {
        let camera = match crate::cameras::repo_get(&self.db, &self.id).await {
            Ok(Some(c)) => c,
            _ => return,
        };
        // Only cameras that can produce detections (today: ONVIF motion).
        let detects = camera.motion.enabled && camera.motion.source == MotionSource::Onvif && camera.onvif.as_ref().is_some_and(|o| !o.url.is_empty());
        if !camera.enabled || camera.recording.mode != RecordingMode::Events || !detects {
            return;
        }
        let plan = Plan::from_settings(&camera.recording);
        // Keeps the stream open and its pre-record buffer filled.
        let _keepalive = self.hub.subscribe_with_preroll(&self.id, plan.stream, plan.preroll_secs + 2);
        let mut events = self.bus.subscribe();
        let mut active: HashSet<EventType> = HashSet::new();
        let mut first_kind = EventType::Motion;
        let mut stop_at: Option<Instant> = None;
        let mut clip_started: Option<Instant> = None;
        let far = Duration::from_secs(365 * 24 * 3600);

        loop {
            let split_at = clip_started.map(|t| t + plan.max_clip);
            let next = [stop_at, split_at].into_iter().flatten().min().unwrap_or_else(|| Instant::now() + far);
            tokio::select! {
                event = events.recv() => match event {
                    Ok(BusEvent::DetectionStarted { camera_id, kind, .. }) if camera_id == self.id => {
                        if active.is_empty() {
                            first_kind = kind;
                        }
                        active.insert(kind);
                        stop_at = None;
                        if self.recorder.running_reason(&self.id).is_none() && self.start(&plan, first_kind) {
                            clip_started = Some(Instant::now());
                        }
                    }
                    Ok(BusEvent::DetectionEnded { camera_id, kind, .. }) if camera_id == self.id => {
                        active.remove(&kind);
                        if active.is_empty() && clip_started.is_some() {
                            stop_at = Some(Instant::now() + plan.hold);
                        }
                    }
                    Ok(BusEvent::RecordingStopped { camera_id, .. }) if camera_id == self.id => {
                        // Stopped by someone else (STOP button, failure): forget it.
                        if self.recorder.running_reason(&self.id).is_none() {
                            clip_started = None;
                            stop_at = None;
                        }
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => return,
                },
                _ = tokio::time::sleep_until(next) => {
                    if stop_at.is_some_and(|t| Instant::now() >= t) {
                        self.recorder.stop_if(&self.id, &OWN).await;
                        stop_at = None;
                        clip_started = None;
                    } else if split_at.is_some_and(|t| Instant::now() >= t) {
                        // Long event: close this clip and continue in a new one.
                        self.recorder.stop_if(&self.id, &OWN).await;
                        clip_started = (!active.is_empty() && self.start(&plan, first_kind)).then(Instant::now);
                    }
                }
            }
        }
    }

    fn start(&self, plan: &Plan, kind: EventType) -> bool {
        let reason = if kind == EventType::Motion { RecordingReason::Motion } else { RecordingReason::Event };
        self.recorder.start_with(&self.id, Spec { reason, stream: plan.stream, preroll_secs: plan.preroll_secs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_uses_camera_settings_with_safe_bounds() {
        let mut r = RecordingSettings::default();
        r.pre_record_seconds = 120;
        r.post_record_seconds = 5;
        r.event_merge_seconds = 10;
        r.max_clip_seconds = 1;
        r.stream = StreamRole::Sub;
        let p = Plan::from_settings(&r);
        assert_eq!(p.preroll_secs, MAX_PREROLL_SECS);
        assert_eq!(p.hold, Duration::from_secs(10), "merge window wins over a shorter post-record");
        assert_eq!(p.max_clip, Duration::from_secs(30));
        assert_eq!(p.stream, StreamKind::Sub);
    }
}
