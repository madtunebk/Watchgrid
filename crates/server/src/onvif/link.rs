//! Whether each camera's ONVIF events actually work — so software motion
//! detection can stand in while they don't (a camera that refuses the
//! subscription, or accepts it and drops it again and again) — and what
//! can be said about them: connected since when, last detection reported.
//!
//! A successful pull proves delivery, not detection: a quiet scene sends
//! nothing, and so does a camera whose motion rules are off. So the status
//! also carries the last reported detection, and says "none yet" honestly.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use watchgrid_model::OnvifEventsStatus;

/// After the watcher starts: time to connect before anything counts as failing.
const GRACE: Duration = Duration::from_secs(60);
/// After a failure, events must flow this long without a drop to count as
/// working again (a camera that accepts and drops at once isn't trusted).
const STABLE: Duration = Duration::from_secs(120);

struct Link {
    watching_since: Instant,
    /// Delivering events since then, without a drop.
    up_since: Option<(Instant, DateTime<Utc>)>,
    /// It has dropped or refused at least once: recovery must prove itself.
    has_failed: bool,
    last_detection: Option<DateTime<Utc>>,
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
        self.with(|m| m.insert(id.to_string(), Link { watching_since: now, up_since: None, has_failed: false, last_detection: None }));
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
                l.up_since.get_or_insert((now, Utc::now()));
            }
        });
    }

    /// The subscription broke (or couldn't be made).
    pub fn lost(&self, id: &str) {
        self.with(|m| {
            if let Some(l) = m.get_mut(id) {
                l.up_since = None;
                l.has_failed = true;
            }
        });
    }

    /// The camera reported a detection (starting or ending).
    pub fn detected(&self, id: &str) {
        self.with(|m| {
            if let Some(l) = m.get_mut(id) {
                l.last_detection = Some(Utc::now());
            }
        });
    }

    /// ONVIF is supposed to report motion but doesn't reliably: nothing
    /// delivered yet after the grace, down now, or back too recently after
    /// a failure.
    pub fn failing(&self, id: &str) -> bool {
        self.failing_at(id, Instant::now())
    }

    fn failing_at(&self, id: &str, now: Instant) -> bool {
        self.with(|m| {
            m.get(id).is_some_and(|l| {
                now.duration_since(l.watching_since) >= GRACE
                    && match l.up_since {
                        None => true,
                        Some((t, _)) => l.has_failed && now.duration_since(t) < STABLE,
                    }
            })
        })
    }

    /// What the UI shows for a camera whose motion comes from ONVIF.
    pub fn status(&self, id: &str) -> Option<OnvifEventsStatus> {
        self.with(|m| {
            m.get(id).map(|l| OnvifEventsStatus { connected: l.up_since.is_some(), connected_since: l.up_since.map(|(_, at)| at), last_detection: l.last_detection })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_camera_is_trusted_and_a_failed_one_must_prove_itself() {
        let links = OnvifLinks::default();
        let start = Instant::now();
        let at = |secs: u64| start + Duration::from_secs(secs);
        assert!(!links.failing_at("cam", at(600)), "not watched: not ONVIF's job");

        links.watching_at("cam", start);
        assert!(!links.failing_at("cam", at(30)), "still connecting");
        assert!(links.failing_at("cam", start + GRACE), "nothing delivered after the grace");

        // Healthy from the start: trusted as soon as it delivers.
        links.delivered_at("cam", at(15));
        assert!(!links.failing_at("cam", start + GRACE));

        // A drop counts at once; coming back must last.
        links.lost("cam");
        assert!(links.failing_at("cam", at(100)));
        links.delivered_at("cam", at(100));
        assert!(links.failing_at("cam", at(130)), "just back after a failure");
        assert!(!links.failing_at("cam", at(100) + STABLE), "stable again");

        links.gone("cam");
        assert!(!links.failing_at("cam", at(1000)));
    }

    #[test]
    fn status_says_connected_and_the_last_detection() {
        let links = OnvifLinks::default();
        links.watching("cam");
        let s = links.status("cam").unwrap();
        assert!(!s.connected && s.last_detection.is_none());
        links.delivered("cam");
        links.detected("cam");
        let s = links.status("cam").unwrap();
        assert!(s.connected && s.connected_since.is_some() && s.last_detection.is_some());
        assert!(links.status("other").is_none());
    }
}
