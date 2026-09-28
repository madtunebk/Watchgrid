//! Automatic recording, one controller task per camera, by recording mode:
//!
//! - events: record while detections are active, with pre-record (buffered
//!   video before the event), post-record, merging and clip splitting;
//! - continuous / scheduled: see [`super::timed`].
//!
//! The controller keeps the camera's recording stream open (filling the
//! pre-record buffer). Manual recordings are never touched.

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
use crate::media::{MediaHub, StreamKind, Subscription};
use crate::motion::Detections;

/// Pre-record is capped to bound memory (frames are kept in RAM).
const MAX_PREROLL_SECS: u32 = watchgrid_model::MAX_PRE_RECORD_SECONDS;
/// Reasons this controller starts, and therefore may stop.
const OWN: [RecordingReason; 2] = [RecordingReason::Motion, RecordingReason::Event];
/// A clip that fails while the detection goes on is started again after
/// 5 s, then 10 s, then 15 s; after that, only the next detection retries.
const RETRY_AFTER: Duration = Duration::from_secs(5);
const MAX_RETRIES: u32 = 3;

pub struct AutoRecorders {
    db: PgPool,
    hub: Arc<MediaHub>,
    bus: Bus,
    recorder: Arc<Recorder>,
    /// The combined detection state, to catch up after missed bus messages.
    detections: Arc<Detections>,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    enabled: bool,
}

impl AutoRecorders {
    pub fn new(db: PgPool, hub: Arc<MediaHub>, bus: Bus, recorder: Arc<Recorder>, detections: Arc<Detections>) -> Self {
        Self { db, hub, bus, recorder, detections, tasks: Mutex::new(HashMap::new()), enabled: true }
    }

    #[cfg(test)]
    pub fn inert(db: PgPool, hub: Arc<MediaHub>, bus: Bus, recorder: Arc<Recorder>, detections: Arc<Detections>) -> Self {
        Self { enabled: false, ..Self::new(db, hub, bus, recorder, detections) }
    }

    /// (Re)start after the camera was added or changed.
    pub fn apply(&self, id: &str, exists: bool) {
        if let Some(old) = self.tasks.lock().expect("auto lock").remove(id) {
            old.abort();
        }
        if exists && self.enabled {
            let ctl = Controller {
                db: self.db.clone(),
                hub: self.hub.clone(),
                bus: self.bus.clone(),
                recorder: self.recorder.clone(),
                detections: self.detections.clone(),
                id: id.to_string(),
            };
            self.tasks.lock().expect("auto lock").insert(id.to_string(), tokio::spawn(ctl.run()));
        }
    }
}

pub(super) struct Controller {
    pub(super) db: PgPool,
    pub(super) hub: Arc<MediaHub>,
    pub(super) bus: Bus,
    pub(super) recorder: Arc<Recorder>,
    pub(super) detections: Arc<Detections>,
    pub(super) id: String,
}

/// What an event-mode controller waits for. Changed by detections, whether
/// heard on the bus or caught up after missed messages, and by the clock.
#[derive(Debug)]
struct Follow {
    active: HashSet<EventType>,
    /// The kind that began the current detection (names the clip's reason).
    first_kind: EventType,
    /// Recording starts once a detection has lasted `min_event`.
    start_at: Option<Instant>,
    stop_at: Option<Instant>,
    clip_started: Option<Instant>,
    /// Failed clips restarted during the current detection.
    retries: u32,
}

impl Follow {
    fn new() -> Self {
        Self { active: HashSet::new(), first_kind: EventType::Motion, start_at: None, stop_at: None, clip_started: None, retries: 0 }
    }

    /// `kind` began; `recording`: something already records this camera.
    fn started(&mut self, kind: EventType, recording: bool, plan: &Plan, now: Instant) {
        if self.active.is_empty() {
            self.first_kind = kind;
        }
        self.active.insert(kind);
        self.stop_at = None;
        if !recording && self.start_at.is_none() && self.clip_started.is_none() {
            self.start_at = Some(now + plan.min_event);
        }
    }

    fn ended(&mut self, kind: EventType, plan: &Plan, now: Instant) {
        self.active.remove(&kind);
        if self.active.is_empty() {
            // Too short to record, if it hadn't started yet.
            self.start_at = None;
            self.retries = 0;
            if self.clip_started.is_some() {
                self.stop_at = Some(now + plan.hold);
            }
        }
    }

    /// Our clip failed (camera dropped, disk error…) while the detection may
    /// still go on: start it again soon, a few times per detection. Returns
    /// the wait, or `None` when nothing is retried.
    fn failed(&mut self, now: Instant) -> Option<Duration> {
        self.clip_started = None;
        self.stop_at = None;
        if self.active.is_empty() || self.retries >= MAX_RETRIES {
            return None;
        }
        self.retries += 1;
        let wait = RETRY_AFTER * self.retries;
        self.start_at = Some(now + wait);
        Some(wait)
    }

    /// Take over an event clip that is already recording (the controller
    /// was restarted): it ends as usual once the detection is over — right
    /// away plus the hold if it already is.
    fn adopt(&mut self, now: Instant, plan: &Plan) {
        self.clip_started = Some(now);
        self.start_at = None;
        if self.active.is_empty() {
            self.stop_at = Some(now + plan.hold);
        }
    }

    /// Bus messages were missed (the bus holds 256): take what the camera
    /// sees now as the truth, as if the missed starts and ends had arrived.
    /// A lost start no longer misses a recording, a lost end no longer
    /// keeps one running.
    fn catch_up(&mut self, truth: &HashSet<EventType>, recording: bool, plan: &Plan, now: Instant) {
        let gone: Vec<EventType> = self.active.difference(truth).copied().collect();
        for kind in gone {
            self.ended(kind, plan, now);
        }
        for kind in truth.difference(&self.active.clone()).copied().collect::<Vec<_>>() {
            self.started(kind, recording, plan, now);
        }
    }
}

/// The settings that matter here, in usable units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub stream: StreamKind,
    pub preroll_secs: u32,
    /// Keep recording this long after the last detection ends.
    pub hold: Duration,
    pub max_clip: Duration,
    /// Detections shorter than this don't start a recording.
    pub min_event: Duration,
}

impl Plan {
    pub fn from_settings(r: &RecordingSettings) -> Self {
        Self {
            stream: if r.stream == StreamRole::Sub { StreamKind::Sub } else { StreamKind::Main },
            preroll_secs: r.pre_record_seconds.min(MAX_PREROLL_SECS),
            // Post-record, but at least the merge window so close events share a clip.
            hold: Duration::from_secs(u64::from(r.post_record_seconds.max(r.event_merge_seconds))),
            max_clip: Duration::from_secs(u64::from(r.max_clip_seconds.max(30))),
            min_event: Duration::from_secs(u64::from(r.min_event_seconds.min(60))),
        }
    }

    /// Pre-record to ask for when starting `min_event` after the detection,
    /// so the clip still begins `preroll_secs` before the detection did.
    pub fn delayed_preroll(&self) -> u32 {
        self.preroll_secs + self.min_event.as_secs() as u32
    }
}

impl Controller {
    async fn run(self) {
        let camera = match crate::cameras::repo_get(&self.db, &self.id).await {
            Ok(Some(c)) if c.enabled => c,
            _ => {
                self.recorder.stop_if(&self.id, &OWN).await;
                return;
            }
        };
        // Only cameras that can produce detections: ONVIF events or
        // Watchgrid's own software motion detection.
        let detects = camera.motion.enabled
            && match camera.motion.source {
                MotionSource::Onvif => camera.onvif.as_ref().is_some_and(|o| !o.url.is_empty()),
                MotionSource::Software => true,
                MotionSource::Ai => false,
            };
        // An event clip the previous controller left running (settings
        // saved, disarmed…): if events no longer drive this camera, nothing
        // would ever end it. Manual recordings are never touched.
        if camera.recording.mode != RecordingMode::Events || !detects {
            self.recorder.stop_if(&self.id, &OWN).await;
        }
        match camera.recording.mode {
            RecordingMode::Events => {}
            RecordingMode::Continuous | RecordingMode::Scheduled => return super::timed::run(&self, &camera).await,
            RecordingMode::Disabled | RecordingMode::Manual => return,
        }
        if !detects {
            return;
        }
        let plan = Plan::from_settings(&camera.recording);
        // The substream is open anyway (camera health): keep its pre-record
        // buffer filled. The main stream is opened only from the moment
        // motion starts (`warm`, below): nothing is pulled from the camera
        // while nothing happens, and a clip starts about when the motion
        // did, not the configured seconds before.
        let _keepalive = (plan.stream == StreamKind::Sub).then(|| self.hub.subscribe_with_preroll(&self.id, StreamKind::Sub, plan.delayed_preroll() + 2));
        let mut warm: Option<Subscription> = None;
        let mut events = self.bus.subscribe();
        let mut f = Follow::new();
        // Detections already under way (the controller was just restarted).
        f.catch_up(&self.detections.active_kinds(&self.id), self.recorder.running_reason(&self.id).is_some(), &plan, Instant::now());
        // …and an event clip still running from before becomes this one's.
        if self.recorder.running_reason(&self.id).is_some_and(|r| OWN.contains(&r)) {
            f.adopt(Instant::now(), &plan);
        }
        let far = Duration::from_secs(365 * 24 * 3600);

        loop {
            if plan.stream == StreamKind::Main {
                // Open while a detection lasts, a start is pending or a clip runs.
                let wanted = !f.active.is_empty() || f.start_at.is_some() || f.clip_started.is_some();
                if wanted && warm.is_none() {
                    warm = Some(self.hub.subscribe_with_preroll(&self.id, StreamKind::Main, plan.delayed_preroll() + 2));
                } else if !wanted {
                    warm = None;
                }
            }
            let split_at = f.clip_started.map(|t| t + plan.max_clip);
            let next = [f.stop_at, split_at, f.start_at].into_iter().flatten().min().unwrap_or_else(|| Instant::now() + far);
            tokio::select! {
                event = events.recv() => match event {
                    Ok(BusEvent::DetectionStarted { camera_id, kind, .. }) if camera_id == self.id => {
                        f.started(kind, self.recorder.running_reason(&self.id).is_some(), &plan, Instant::now());
                    }
                    Ok(BusEvent::DetectionEnded { camera_id, kind, .. }) if camera_id == self.id => f.ended(kind, &plan, Instant::now()),
                    Ok(BusEvent::RecordingStopped { camera_id, error, .. }) if camera_id == self.id => {
                        if self.recorder.running_reason(&self.id).is_none() {
                            let ours = f.clip_started.is_some();
                            match error {
                                // Our clip failed: retry while the detection lasts.
                                Some(e) if ours => match f.failed(Instant::now()) {
                                    Some(wait) => tracing::warn!(camera = %self.id, attempt = f.retries, retry_in_s = wait.as_secs(), "event recording failed while the detection continues, starting it again: {e}"),
                                    None if !f.active.is_empty() => tracing::warn!(camera = %self.id, "event recording failed again; the next detection will try: {e}"),
                                    None => {}
                                },
                                // Stopped by someone else (STOP button): forget it.
                                _ => {
                                    f.clip_started = None;
                                    f.stop_at = None;
                                }
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(missed)) => {
                        let truth = self.detections.active_kinds(&self.id);
                        tracing::warn!(camera = %self.id, missed, active = ?truth, "auto recording missed bus messages; caught up from the detection state");
                        f.catch_up(&truth, self.recorder.running_reason(&self.id).is_some(), &plan, Instant::now());
                    }
                    Err(RecvError::Closed) => return,
                },
                _ = tokio::time::sleep_until(next) => {
                    if f.start_at.is_some_and(|t| Instant::now() >= t) {
                        f.start_at = None;
                        if !f.active.is_empty() && self.recorder.running_reason(&self.id).is_none() && self.start(&plan, f.first_kind, plan.delayed_preroll()) {
                            f.clip_started = Some(Instant::now());
                        }
                    } else if f.stop_at.is_some_and(|t| Instant::now() >= t) {
                        self.recorder.stop_if(&self.id, &OWN).await;
                        f.stop_at = None;
                        f.clip_started = None;
                    } else if split_at.is_some_and(|t| Instant::now() >= t) {
                        // Long event: close this clip and continue in a new one.
                        self.recorder.stop_if(&self.id, &OWN).await;
                        f.clip_started = (!f.active.is_empty() && self.start(&plan, f.first_kind, plan.preroll_secs)).then(Instant::now);
                    }
                }
            }
        }
    }

    fn start(&self, plan: &Plan, kind: EventType, preroll_secs: u32) -> bool {
        let reason = if kind == EventType::Motion { RecordingReason::Motion } else { RecordingReason::Event };
        self.recorder.start_with(&self.id, Spec { reason, stream: plan.stream, preroll_secs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> Plan {
        Plan::from_settings(&RecordingSettings { min_event_seconds: 2, post_record_seconds: 10, ..RecordingSettings::default() })
    }

    fn kinds(k: &[EventType]) -> HashSet<EventType> {
        k.iter().copied().collect()
    }

    #[test]
    fn a_failed_clip_is_retried_while_the_detection_lasts() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.started(EventType::Motion, false, &p, now);
        f.clip_started = Some(now);
        assert_eq!(f.failed(now), Some(Duration::from_secs(5)));
        assert_eq!(f.start_at, Some(now + Duration::from_secs(5)));
        assert_eq!(f.failed(now), Some(Duration::from_secs(10)), "waits longer each time");
        assert_eq!(f.failed(now), Some(Duration::from_secs(15)));
        assert_eq!(f.failed(now), None, "three retries per detection");
        // The detection ends: the next one gets its own retries.
        f.ended(EventType::Motion, &p, now);
        f.started(EventType::Motion, false, &p, now);
        assert_eq!(f.failed(now), Some(Duration::from_secs(5)));
    }

    #[test]
    fn a_failure_after_the_detection_ended_is_not_retried() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.started(EventType::Motion, false, &p, now);
        f.clip_started = Some(now);
        f.ended(EventType::Motion, &p, now);
        assert_eq!(f.failed(now), None);
        assert_eq!((f.start_at, f.stop_at, f.clip_started), (None, None, None));
    }

    #[test]
    fn an_adopted_clip_still_ends() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.adopt(now, &p);
        assert_eq!((f.clip_started, f.stop_at), (Some(now), Some(now + p.hold)), "motion already over: it ends after the hold");

        let mut f = Follow::new();
        f.catch_up(&kinds(&[EventType::Motion]), true, &p, now);
        f.adopt(now, &p);
        assert_eq!(f.stop_at, None, "motion still going: it keeps recording");
        f.ended(EventType::Motion, &p, now);
        assert_eq!(f.stop_at, Some(now + p.hold), "and ends when the motion does");
    }

    #[test]
    fn a_lost_start_still_records() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.catch_up(&kinds(&[EventType::Person]), false, &p, now);
        assert_eq!(f.start_at, Some(now + p.min_event), "recording scheduled as if the start had arrived");
        assert_eq!(f.first_kind, EventType::Person);
    }

    #[test]
    fn a_lost_end_stops_the_recording() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.started(EventType::Motion, false, &p, now);
        f.start_at = None;
        f.clip_started = Some(now); // it recorded
        f.catch_up(&kinds(&[]), true, &p, now);
        assert!(f.active.is_empty());
        assert_eq!(f.stop_at, Some(now + p.hold), "stops after the usual hold, not never");
    }

    #[test]
    fn a_lost_end_before_recording_cancels_it() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.started(EventType::Motion, false, &p, now);
        f.catch_up(&kinds(&[]), false, &p, now);
        assert_eq!((f.start_at, f.stop_at), (None, None), "too short to record, as usual");
    }

    #[test]
    fn nothing_missed_changes_nothing() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.started(EventType::Motion, false, &p, now);
        let before = (f.start_at, f.stop_at, f.clip_started);
        f.catch_up(&kinds(&[EventType::Motion]), false, &p, now + Duration::from_secs(1));
        assert_eq!((f.start_at, f.stop_at, f.clip_started), before, "the pending start keeps its time");
    }

    #[test]
    fn a_caught_up_start_respects_a_running_recording() {
        let (p, now) = (plan(), Instant::now());
        let mut f = Follow::new();
        f.catch_up(&kinds(&[EventType::Motion]), true, &p, now);
        assert_eq!(f.start_at, None, "a manual or continuous recording already runs");
        assert!(f.active.contains(&EventType::Motion));
    }

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
        r.min_event_seconds = 3;
        r.pre_record_seconds = 5;
        let p = Plan::from_settings(&r);
        assert_eq!(p.min_event, Duration::from_secs(3));
        assert_eq!(p.delayed_preroll(), 8, "starting 3 s late still keeps 5 s before the detection");
    }
}
