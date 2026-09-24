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

pub struct Watchers {
    deps: Deps,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
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
        if let Some(old) = self.tasks.lock().expect("watchers lock").remove(id) {
            old.abort();
        }
        if exists && self.enabled {
            let handle = tokio::spawn(run(self.deps.clone(), id.to_string()));
            self.tasks.lock().expect("watchers lock").insert(id.to_string(), handle);
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

async fn run(deps: Deps, id: String) {
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
                let (reason, delivered) = pull_until_error(&deps, &id, &sub, &mut state, attempt > 0).await;
                if delivered {
                    attempt = 0;
                }
                sub.unsubscribe().await;
                reason
            }
        };
        // Without events we can't know when detections end: close them now.
        for kind in state.active_kinds() {
            deps.bus.publish(BusEvent::DetectionEnded { camera_id: id.clone(), kind, at: Utc::now() });
        }
        state = Detections::default();
        set_motion(&deps, &id, false);
        attempt += 1;
        if attempt == 1 || attempt.is_multiple_of(10) {
            tracing::warn!(camera = %id, "ONVIF events unavailable: {reason}");
        }
        tokio::time::sleep(backoff::delay(attempt)).await;
    }
}

/// Returns why pulling stopped and whether any pull succeeded.
async fn pull_until_error(deps: &Deps, id: &str, sub: &Subscription, state: &mut Detections, recovering: bool) -> (String, bool) {
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
            }
            Err(e) => return (e, delivered),
        }
    }
}

/// Turn one notification into bus transitions (per event type, so several
/// motion-like topics count as one motion).
fn apply(deps: &Deps, id: &str, state: &mut Detections, n: &Notification) {
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
}
