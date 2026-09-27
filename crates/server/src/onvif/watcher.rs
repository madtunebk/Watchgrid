//! One task per camera that takes motion from ONVIF events: subscribe
//! (PullPoint), pull continuously, renew, reconnect with backoff. Topic
//! states become DetectionStarted/Ended on the bus and the live MOTION flag.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::watch;
use watchgrid_model::EventType;

use super::endpoint::renew_after;
use super::pullpoint::{self, Subscription};
use super::{Notification, topics};
use crate::bus::{Bus, BusEvent};
use crate::cameras;
use crate::credentials::CredentialStore;
use crate::live::LiveRegistry;
use crate::motion::{Detections as Combined, Source};

/// A stopped watcher gets this long to unsubscribe (freeing the camera's
/// slot now instead of when the lease runs out).
const UNSUBSCRIBE_LIMIT: Duration = Duration::from_secs(3);
/// At most one pull per this long: some cameras answer at once instead of
/// waiting, and a tight loop of requests gets a camera to refuse us.
const PULL_EVERY: Duration = Duration::from_secs(1);
/// A connection that delivered this long counts as working: only then does
/// the reconnect delay start over. (A camera that accepts and drops again
/// right away must not be re-subscribed every few seconds.)
const STABLE: Duration = Duration::from_secs(60);
/// Each reconnect creates a subscription on the camera, which keeps it for
/// its whole lifetime even when the connection dropped: Tapo allows only a
/// few and then refuses everything (HTTP 400, `error_code -40210`).
const RETRY_FIRST: Duration = Duration::from_secs(10);
const RETRY_MAX: Duration = Duration::from_secs(300);

/// Wait before reconnect attempt `attempt` (1-based): 10 s, doubling, 5 min at most.
fn retry_delay(attempt: u32) -> Duration {
    RETRY_FIRST.saturating_mul(2u32.saturating_pow(attempt.saturating_sub(1).min(8))).min(RETRY_MAX)
}

#[derive(Clone)]
pub struct Deps {
    pub db: PgPool,
    pub credentials: Arc<CredentialStore>,
    pub bus: Bus,
    /// Whether events flow (software motion stands in while they don't).
    pub links: Arc<super::OnvifLinks>,
    /// Where detections go (combined with software detection's).
    pub detections: Arc<Combined>,
}

/// Detection types in progress per camera, shared with its task so a
/// replaced task's open detections can be closed.
type Open = Arc<Mutex<Vec<EventType>>>;

/// Set to `true` (or dropped) to stop a watcher.
type Stop = watch::Receiver<bool>;

pub struct Watchers {
    deps: Deps,
    tasks: Mutex<HashMap<String, (watch::Sender<bool>, Open)>>,
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

    /// Shared with software motion detection (it stands in while failing).
    pub fn links(&self) -> Arc<super::OnvifLinks> {
        self.deps.links.clone()
    }

    /// Shared with software motion detection: one combined state per camera.
    pub fn detections(&self) -> Arc<Combined> {
        self.deps.detections.clone()
    }

    /// (Re)start watching after the camera was added or changed; the task
    /// itself decides from the settings whether there is anything to do.
    pub fn apply(&self, id: &str, exists: bool) {
        if let Some((stop, open)) = self.tasks.lock().expect("watchers lock").remove(id) {
            // Not aborted: it unsubscribes first (Tapo keeps a dropped
            // subscription until its lease ends and allows only a few).
            let _ = stop.send(true);
            self.deps.links.gone(id);
            // Stopped mid-detection (motion switched off, settings saved):
            // close what was open, or MOTION and the event stay on forever.
            let open = open.lock().expect("detections lock").clone();
            if !open.is_empty() {
                close_all(&self.deps, id, &open);
            }
        }
        if exists && self.enabled {
            let open = Open::default();
            let (stop, stopped) = watch::channel(false);
            tokio::spawn(run(self.deps.clone(), id.to_string(), open.clone(), stopped));
            self.tasks.lock().expect("watchers lock").insert(id.to_string(), (stop, open));
        }
    }
}

/// Current state of each property (topic + source, see
/// [`Notification::identity`]) and what it maps to.
struct Detections {
    topics: HashMap<String, (EventType, bool)>,
    /// Detections count (motion comes from ONVIF); security alerts always do.
    motion: bool,
}

impl Default for Detections {
    fn default() -> Self {
        Self { topics: HashMap::new(), motion: true }
    }
}

impl Detections {
    fn kind_active(&self, kind: EventType) -> bool {
        self.topics.values().any(|(k, on)| *k == kind && *on)
    }

    fn active_kinds(&self) -> Vec<EventType> {
        let mut kinds: Vec<EventType> = self.topics.values().filter(|(_, on)| *on).map(|(k, _)| *k).collect();
        kinds.dedup();
        kinds
    }
}

/// Let the RTSP stream connect first: some Tapo firmware (TC71) delivers an
/// ONVIF reply on the RTSP connection when both are opened at the same
/// moment, which drops the stream (seen at every service start).
const START_DELAY: Duration = Duration::from_secs(10);

/// `f`, unless the watcher is stopped first (`None`).
async fn unless_stopped<F: std::future::Future>(stop: &mut Stop, f: F) -> Option<F::Output> {
    tokio::select! {
        out = f => Some(out),
        _ = stop.wait_for(|s| *s) => None,
    }
}

async fn run(deps: Deps, id: String, open: Open, mut stop: Stop) {
    // Whoever stopped us closed our detections and link: just leave.
    if unless_stopped(&mut stop, tokio::time::sleep(START_DELAY)).await.is_none() {
        return;
    }
    let mut attempt = 0u32;
    let mut state = Detections::default();
    let mut registered = false;
    loop {
        let Some(cfg) = unless_stopped(&mut stop, cameras::onvif_watch(&deps.db, &deps.credentials, &id)).await else { return };
        let cfg = match cfg {
            Ok(Some(cfg)) if cfg.active => cfg,
            Ok(_) => {
                deps.links.gone(&id);
                return; // gone, disabled, or without ONVIF
            }
            Err(e) => {
                tracing::warn!(camera = %id, "cannot load ONVIF settings: {e}");
                if unless_stopped(&mut stop, tokio::time::sleep(retry_delay(attempt.max(1)))).await.is_none() {
                    return;
                }
                continue;
            }
        };
        state.motion = cfg.motion;
        // Only cameras whose motion comes from ONVIF get a stand-in.
        if cfg.motion && !registered {
            deps.links.watching(&id);
            registered = true;
        }
        let Some(created) = unless_stopped(&mut stop, Subscription::create(&cfg.url, &cfg.username, cfg.password)).await else { return };
        let reason = match created {
            Err(e) => e,
            Ok(sub) => {
                tracing::info!(camera = %id, lease_s = sub.lease().as_secs(), renew_in_s = renew_after(sub.lease()).as_secs(), reference_parameters = sub.reference_parameters(), "ONVIF subscribed");
                // Only a working pull counts as connected: some cameras
                // accept the subscription but then refuse to deliver.
                let pulling = pull_until_error(&deps, &id, &sub, &mut state, &open, attempt > 0);
                let Some((reason, delivered)) = unless_stopped(&mut stop, pulling).await else {
                    // Settings saved or camera removed: free its slot now.
                    let _ = tokio::time::timeout(UNSUBSCRIBE_LIMIT, sub.unsubscribe()).await;
                    return;
                };
                if delivered.is_some_and(|d| d >= STABLE) {
                    attempt = 0;
                }
                sub.unsubscribe().await;
                reason
            }
        };
        deps.links.lost(&id);
        // Without events we can't know when detections end: close them now.
        close_all(&deps, &id, &state.active_kinds());
        state = Detections { motion: state.motion, ..Detections::default() };
        open.lock().expect("detections lock").clear();
        attempt += 1;
        // Reconnects are at least 10 s apart: every failure is worth a line.
        tracing::warn!(camera = %id, attempt, retry_in_s = retry_delay(attempt).as_secs(), "ONVIF events unavailable: {reason}");
        if unless_stopped(&mut stop, tokio::time::sleep(retry_delay(attempt))).await.is_none() {
            return;
        }
    }
}

/// Returns why pulling stopped and, if any pull succeeded, for how long
/// events were delivered.
async fn pull_until_error(deps: &Deps, id: &str, sub: &Subscription, state: &mut Detections, open: &Open, recovering: bool) -> (String, Option<Duration>) {
    let mut renew_at = Instant::now() + renew_after(sub.lease());
    let mut first_ok: Option<Instant> = None;
    loop {
        let delivered = first_ok.map(|t| t.elapsed());
        if Instant::now() >= renew_at {
            match sub.renew().await {
                // The camera may grant less than asked, or less than before.
                Ok(lease) => renew_at = Instant::now() + renew_after(lease),
                Err(e) => return (format!("renew failed: {e}"), delivered),
            }
        }
        let started = Instant::now();
        match sub.pull().await {
            Ok(list) => {
                if first_ok.is_none() && recovering {
                    tracing::info!(camera = %id, "ONVIF events connected");
                }
                first_ok.get_or_insert(started);
                deps.links.delivered(id);
                for n in list {
                    apply(deps, id, state, &n);
                }
                *open.lock().expect("detections lock") = state.active_kinds();
                // The camera answered at once: don't ask again straight away.
                if let Some(rest) = PULL_EVERY.checked_sub(started.elapsed()) {
                    tokio::time::sleep(rest).await;
                }
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
    let (true, Some(kind)) = (state.motion, topics::detection(&n.topic)) else { return };
    // Proof that detection reaches us, not just that pulls succeed.
    deps.links.detected(id);
    let was = state.kind_active(kind);
    if n.deleted() {
        // Gone (no Data follows): it no longer holds anything on.
        state.topics.remove(&n.identity());
    } else {
        let Some(on) = n.active() else { return };
        state.topics.insert(n.identity(), (kind, on));
    }
    let now = state.kind_active(kind);
    let at = pullpoint::when(n);
    if now && !was {
        deps.detections.start(id, kind, Source::Camera, &n.topic, at);
    } else if was && !now {
        deps.detections.end(id, kind, Source::Camera, at);
    }
}

/// End every detection in `kinds` that the camera reported (the MOTION
/// badge follows the combined state).
fn close_all(deps: &Deps, id: &str, kinds: &[EventType]) {
    for kind in kinds {
        deps.detections.end(id, *kind, Source::Camera, Utc::now());
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_stop_interrupts_whatever_the_watcher_waits_on() {
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        assert_eq!(super::unless_stopped(&mut stopped, async { 7 }).await, Some(7));
        let _ = stop.send(true);
        assert_eq!(super::unless_stopped(&mut stopped, std::future::pending::<()>()).await, None);
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        drop(stop); // the watchers map let go of it
        assert_eq!(super::unless_stopped(&mut stopped, std::future::pending::<()>()).await, None);
    }

    #[test]
    fn reconnects_slow_down_and_never_hurry() {
        assert_eq!(super::retry_delay(1), Duration::from_secs(10));
        assert_eq!(super::retry_delay(2), Duration::from_secs(20));
        assert_eq!(super::retry_delay(5), Duration::from_secs(160));
        assert_eq!(super::retry_delay(6), super::RETRY_MAX);
        assert_eq!(super::retry_delay(40), super::RETRY_MAX);
    }

    use super::*;

    fn note(topic: &str, on: bool) -> Notification {
        Notification { topic: topic.into(), time: None, operation: "Changed".into(), data: vec![("State".into(), on.to_string())], source: vec![] }
    }

    #[tokio::test]
    async fn replacing_a_watcher_closes_its_open_detections() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let combined = Arc::new(Combined::new(bus.clone(), Arc::new(LiveRegistry::default())));
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            bus: bus.clone(),
            links: Arc::default(),
            detections: combined.clone(),
        };
        let watchers = Watchers::inert(deps);
        // The camera reported motion (as the watcher's `apply` would).
        combined.start("cam", EventType::Motion, Source::Camera, "m", Utc::now());
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionStarted { .. }));
        let open: Open = Arc::new(Mutex::new(vec![EventType::Motion]));
        watchers.tasks.lock().unwrap().insert("cam".into(), (tokio::sync::watch::channel(false).0, open));
        // e.g. motion switched off while the camera reported motion
        watchers.apply("cam", true);
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionEnded { kind: EventType::Motion, .. }));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn two_rules_on_one_topic_stay_apart_and_deleted_ends_one() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            bus: bus.clone(),
            links: Arc::default(),
            detections: Arc::new(Combined::new(bus, Arc::new(LiveRegistry::default()))),
        };
        let ruled = |rule: &str, op: &str, on: Option<bool>| Notification {
            topic: "RuleEngine/CellMotionDetector/Motion".into(),
            time: None,
            operation: op.into(),
            data: on.map(|b| vec![("IsMotion".to_string(), b.to_string())]).unwrap_or_default(),
            source: vec![("Rule".into(), rule.into())],
        };
        let mut s = Detections { motion: true, ..Detections::default() };
        apply(&deps, "cam", &mut s, &ruled("A", "Changed", Some(true)));
        apply(&deps, "cam", &mut s, &ruled("B", "Changed", Some(false))); // B's "off" must not end A
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionStarted { kind: EventType::Motion, .. }));
        assert!(rx.try_recv().is_err(), "rule A still sees motion");
        apply(&deps, "cam", &mut s, &ruled("A", "Deleted", None)); // rule A removed, no Data
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::DetectionEnded { kind: EventType::Motion, .. }));
    }

    #[tokio::test]
    async fn topics_merge_into_one_transition_per_type() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            bus: bus.clone(),
            links: Arc::default(),
            detections: Arc::new(Combined::new(bus, Arc::new(LiveRegistry::default()))),
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
            bus: bus.clone(),
            links: Arc::default(),
            detections: Arc::new(Combined::new(bus, Arc::new(LiveRegistry::default()))),
        };
        let mut s = Detections::default();
        apply(&deps, "cam", &mut s, &note("UserAlarm/IllegalAccess", true));
        apply(&deps, "cam", &mut s, &note("UserAlarm/IllegalAccess", false));
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::SecurityAlert { .. }));
        assert!(rx.try_recv().is_err(), "the end of the alarm is not another alert, nor a detection");
    }

    #[tokio::test]
    async fn without_onvif_motion_only_security_alerts_count() {
        let bus = Bus::new();
        let mut rx = bus.subscribe();
        let deps = Deps {
            db: sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap(),
            credentials: Arc::new(CredentialStore::from_key(&[1u8; 32])),
            bus: bus.clone(),
            links: Arc::default(),
            detections: Arc::new(Combined::new(bus, Arc::new(LiveRegistry::default()))),
        };
        let mut s = Detections { motion: false, ..Detections::default() };
        apply(&deps, "cam", &mut s, &note("RuleEngine/CellMotionDetector/Motion", true));
        apply(&deps, "cam", &mut s, &note("UserAlarm/IllegalAccess", true));
        assert!(matches!(rx.try_recv().unwrap(), BusEvent::SecurityAlert { .. }));
        assert!(rx.try_recv().is_err(), "motion is off: no detection");
    }
}
