//! Whether each camera's ONVIF events actually work — so software motion
//! detection can stand in while they don't (a camera that refuses the
//! subscription, or accepts it and drops it again and again).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// After the watcher starts: time to connect before anything counts as failing.
const GRACE: Duration = Duration::from_secs(180);
/// Events must flow this long without a drop to count as working again.
const STABLE: Duration = Duration::from_secs(120);

struct Link {
    watching_since: Instant,
    /// Delivering events since then, without a drop.
    up_since: Option<Instant>,
}

#[derive(Default)]
pub struct OnvifLinks(Mutex<HashMap<String, Link>>);

impl OnvifLinks {
    fn with<R>(&self, f: impl FnOnce(&mut HashMap<String, Link>) -> R) -> R {
        f(&mut self.0.lock().expect("onvif links lock"))
    }

    /// The camera takes motion from ONVIF (its watcher started).
    pub fn watching(&self, id: &str) {
        self.watching_at(id, Instant::now());
    }

    fn watching_at(&self, id: &str, now: Instant) {
        self.with(|m| m.insert(id.to_string(), Link { watching_since: now, up_since: None }));
    }

    /// No ONVIF watcher any more (removed, disabled, no ONVIF configured).
    pub fn gone(&self, id: &str) {
        self.with(|m| m.remove(id));
    }

    /// A pull succeeded.
    pub fn delivered(&self, id: &str) {
        self.delivered_at(id, Instant::now());
    }

    fn delivered_at(&self, id: &str, now: Instant) {
        self.with(|m| {
            if let Some(l) = m.get_mut(id) {
                l.up_since.get_or_insert(now);
            }
        });
    }

    /// The subscription broke.
    pub fn lost(&self, id: &str) {
        self.with(|m| {
            if let Some(l) = m.get_mut(id) {
                l.up_since = None;
            }
        });
    }

    /// ONVIF is supposed to report motion but doesn't reliably.
    pub fn failing(&self, id: &str) -> bool {
        self.failing_at(id, Instant::now())
    }

    fn failing_at(&self, id: &str, now: Instant) -> bool {
        self.with(|m| {
            m.get(id).is_some_and(|l| now.duration_since(l.watching_since) >= GRACE && l.up_since.is_none_or(|t| now.duration_since(t) < STABLE))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failing_needs_the_grace_and_recovery_needs_stable_events() {
        let links = OnvifLinks::default();
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        assert!(!links.failing_at("cam", at(600)), "not watched: not ONVIF's job");
        links.watching_at("cam", start);
        assert!(!links.failing_at("cam", at(60)), "still connecting");
        assert!(links.failing_at("cam", start + GRACE), "never delivered");

        links.delivered_at("cam", at(200));
        assert!(links.failing_at("cam", at(230)), "just came up: not trusted yet");
        assert!(!links.failing_at("cam", at(200) + STABLE), "stable again");
        links.delivered_at("cam", at(400));
        assert!(!links.failing_at("cam", at(400)), "more events keep the first up time");

        links.lost("cam");
        assert!(links.failing_at("cam", at(401)), "a drop counts at once");
        links.gone("cam");
        assert!(!links.failing_at("cam", at(500)));
    }
}
