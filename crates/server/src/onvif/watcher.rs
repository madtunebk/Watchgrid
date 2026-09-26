//! One task per camera that takes motion from ONVIF events: subscribe
//! (PullPoint), pull continuously, renew, reconnect with backoff. Topic
//! states become DetectionStarted/Ended on the bus and the live MOTION flag.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::PgPool;
use tokio::task::JoinHandle;
use watchgrid_model::EventType;

use super::pullpoint::{self, Subscription};
use super::{Notification, topics};
use crate::bus::{Bus, BusEvent};
use crate::cameras;
use crate::credentials::CredentialStore;
use crate::live::LiveRegistry;
use crate::supervisor::backoff;

/// Renew well before the subscription's lifetime ends.
const RENEW_EVERY: Duration = Duration::from_secs(pullpoint::LIFETIME_SECS as u64 / 2);

#[derive(Clone)]
pub struct Deps {
    pub db: PgPool,
    pub credentials: Arc<CredentialStore>,
    pub live: Arc<LiveRegistry>,
    pub bus: Bus,
}

/// Detection types in progress per camera, shared with its task so a
/// replaced task's open detections can be closed.
type Open = Arc<Mutex<Vec<EventType>>>;

pub struct Watchers {
    deps: Deps,
    tasks: Mutex<HashMap<String, (JoinHandle<()>, Open)>>,
    /// Tests never contact cameras.
    enabled: bool,
}

impl Watchers {
    pub fn new(deps: Deps) -> Self {
        Self { deps, tasks: Mutex::new(HashMap::new()), enabled: true }
    }

    #[cfg(test)]
    pub fn inert(deps: Deps) -> Self {
        Self { enabled: false, ..Self::new(deps) }
    }

    /// (Re)start watching after the camera was added or changed; the task
    /// itself decides from the settings whether there is anything to do.
    pub fn apply(&self, id: &str, exists: bool) {
        if let Some((old, open)) = self.tasks.lock().expect("watchers lock").remove(id) {
            old.abort();
            // Stopped mid-detection (motion switched off, settings saved):
            // close what was open, or MOTION and the event stay on forever.
            let open = open.lock().expect("detections lock").clone();
            if !open.is_empty() {
                close_all(&self.deps, id, &open);
            }
        }
        if exists && self.enabled {
            let open = Open::default();
            let handle = tokio::spawn(run(self.deps.clone(), id.to_string(), open.clone()));
            self.tasks.lock().expect("watchers lock").insert(id.to_string(), (handle, open));
        }
    }
}

/// Current state of each topic and what it maps to.
#[derive(Default)]
struct Detections(HashMap<String, (EventType, bool)>);

impl Detections {
    fn kind_active(&self, kind: EventType) -> bool {
        self.0.values().any(|(k, on)| *k == kind && *on)
    }

    fn active_kinds(&self) -> Vec<EventType> {
        let mut kinds: Vec<EventType> = self.0.values().filter(|(_, on)| *on).map(|(k, _)| *k).collect();
        kinds.dedup();
        kinds
    }
}

/// Let the RTSP stream connect first: some Tapo firmware (TC71) delivers an
/// ONVIF reply on the RTSP connection when both are opened at the same
/// moment, which drops the stream (seen at every service start).
const START_DELAY: Duration = Duration::from_secs(10);

async fn run(deps: Deps, id: String, open: Open) {
    tokio::time::sleep(START_DELAY).await;
    let mut attempt = 0u32;
    let mut state = Detections::default();
    loop {
        let cfg = match cameras::onvif_watch(&deps.db, &deps.credentials, &id).await {
            Ok(Some(cfg)) if cfg.active => cfg,
            Ok(_) => return, // gone, disabled, or motion not from ONVIF
            Err(e) => {
                tracing::warn!(camera = %id, "cannot load ONVIF settings: {e}");
                tokio::time::sleep(backoff::delay(attempt.max(1))).await;
                continue;
            }
        };
        let reason = match Subscription::create(&cfg.url, &cfg.username, cfg.password).await {
            Err(e) => e,
            Ok(sub) => {
                // Only a working pull counts as connected: some cameras
                // accept the subscription but then refuse to deliver.
                let (reason, delivered) = pull_until_error(&deps, &id, &sub, &mut state, &open, attempt > 0).await;
                if delivered {
                    attempt = 0;
                }
                sub.unsubscribe().await;
                reason
            }
        };
        // Without events we can't know when detections end: close them now.
        close_all(&deps, &id, &state.active_kinds());
        state = Detections::default();
        open.lock().expect("detections lock").clear();
        attempt += 1;
        if attempt == 1 || attempt.is_multiple_of(10) {
            tracing::warn!(camera = %id, "ONVIF events unavailable: {reason}");
        }
        tokio::time::sleep(backoff::delay(attempt)).await;
    }
}

/// Returns why pulling stopped and whether any pull succeeded.
async fn pull_until_error(deps: &Deps, id: &str, sub: &Subscription, state: &mut Detections, open: &Open, recovering: bool) -> (String, bool) {
    let mut renew_at = Instant::now() + RENEW_EVERY;
    let mut delivered = false;
    loop {
        if Instant::now() >= renew_at {
            if let Err(e) = sub.renew().await {
                return (format!("renew failed: {e}"), delivered);
            }
            renew_at = Instant::now() + RENEW_EVERY;
        }
        match sub.pull().await {
            Ok(list) => {
                if !delivered && recovering {
                    tracing::info!(camera = %id, "ONVIF events connected");
                }
                delivered = true;
                for n in list {
                    apply(deps, id, state, &n);
                }
                *open.lock().expect("detections lock") = state.active_kinds();
            }
            Err(e) => return (e, delivered),
        }
    }
}

/// Turn one notification into bus transitions (per event type, so several
/// motion-like topics count as one motion).
fn apply(deps: &Deps, id: &str, state: &mut Detections, n: &Notification) {
    if topics::security(&n.topic) {
        if n.active() == Some(true) {
            deps.bus.publish(BusEvent::SecurityAlert { camera_id: id.to_string(), topic: n.topic.clone(), at: pullpoint::when(n) });
        }
        return;
    }
    let (Some(kind), Some(on)) = (topics::detection(&n.topic), n.active()) else { return };
    let was = state.kind_active(kind);
    state.0.insert(n.topic.clone(), (kind, on));
    let now = state.kind_active(kind);
    let at = pullpoint::when(n);
    if now && !was {
        deps.bus.publish(BusEvent::DetectionStarted { camera_id: id.to_string(), kind, topic: n.topic.clone(), at });
    } else if was && !now {
        deps.bus.publish(BusEvent::DetectionEnded { camera_id: id.to_string(), kind, at });
    }
    // The camera's MOTION badge shows any detection in progress.
    set_motion(deps, id, !state.active_kinds().is_empty());
}

/// End every detection in `kinds` and clear the MOTION badge.
fn close_all(deps: &Deps, id: &str, kinds: &[EventType]) {
    for kind in kinds {
        deps.bus.publish(BusEvent::DetectionEnded { camera_id: id.to_string(), kind: *kind, at: Utc::now() });
    }
    set_motion(deps, id, false);
}

fn set_motion(deps: &Deps, id: &str, on: bool) {
    deps.live.update(id, |l| l.motion_active = on);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(topic: &str, on: bool) -> Notification {
        Notification { topic: topic.into(), time: None, operation: "Changed".into(), data: vec![("State".into(), on.to_string())] }
    }

    #[tokio::test]
    async fn replacing_a_watcher_closes_its_open_detections() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let live = Arc::new(LiveRegistry::default());
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            live: live.clone(),
            bus,
        };
        let watchers = Watchers::inert(deps);
        let open: Open = Arc::new(Mutex::new(vec![EventType::Motion]));
        watchers.tasks.lock().unwrap().insert("cam".into(), (tokio::spawn(async {}), open));
        // e.g. motion switched off while the camera reported motion
        watchers.apply("cam", true);
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionEnded { kind: EventType::Motion, .. }));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn topics_merge_into_one_transition_per_type() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            live: Arc::new(LiveRegistry::default()),
            bus,
        };
        let mut s = Detections::default();
        apply(&deps, "cam", &mut s, &note("RuleEngine/CellMotionDetector/Motion", true));
        apply(&deps, "cam", &mut s, &note("RuleEngine/IntrusionDetector/Intrusion", true)); // still one motion
        apply(&deps, "cam", &mut s, &note("RuleEngine/CellMotionDetector/Motion", false)); // intrusion keeps it on
        apply(&deps, "cam", &mut s, &note("RuleEngine/IntrusionDetector/Intrusion", false));
        apply(&deps, "cam", &mut s, &note("RuleEngine/TamperDetector/Tamper", true)); // not a detection type

        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionStarted { kind: EventType::Motion, .. }));
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionEnded { kind: EventType::Motion, .. }));
        assert!(rx.try_recv().is_err(), "exactly one start and one end");
    }

    #[tokio::test]
    async fn illegal_access_raises_one_security_alert() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            live: Arc::new(LiveRegistry::default()),
            bus,
        };
        let mut s = Detections::default();
        apply(&deps, "cam", &mut s, &note("UserAlarm/IllegalAccess", true));
        apply(&deps, "cam", &mut s, &note("UserAlarm/IllegalAccess", false));
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::SecurityAlert { .. }));
        assert!(rx.try_recv().is_err(), "the end of the alarm is not another alert, nor a detection");
    }
}
