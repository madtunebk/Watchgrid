//! One place that knows whether a camera sees motion, whatever reports it.
//!
//! Camera events (ONVIF) and Watchgrid's own detection can both be running
//! for a camera (the software fallback overlaps ONVIF while it recovers).
//! Each reports its own starts and ends here; the bus hears a start only
//! when the first source starts a kind, and an end only when the last one
//! ends it — so one source ending never cuts a detection (and its
//! recording) the other still sees. The MOTION badge follows the same
//! combined state.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use watchgrid_model::EventType;

use crate::bus::{Bus, BusEvent};
use crate::live::LiveRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// The camera's own events (ONVIF).
    Camera,
    /// Watchgrid's decoding of the substream.
    Software,
}

/// Which sources see which kind, per camera.
type Active = HashMap<String, HashMap<EventType, HashSet<Source>>>;

pub struct Detections {
    bus: Bus,
    live: Arc<LiveRegistry>,
    active: Mutex<Active>,
}

impl Detections {
    pub fn new(bus: Bus, live: Arc<LiveRegistry>) -> Self {
        Self { bus, live, active: Mutex::default() }
    }

    /// `source` sees `kind` from `at` on.
    pub fn start(&self, camera: &str, kind: EventType, source: Source, topic: &str, at: DateTime<Utc>) {
        let first = {
            let mut active = self.active.lock().expect("detections lock");
            let sources = active.entry(camera.to_string()).or_default().entry(kind).or_default();
            let first = sources.is_empty();
            sources.insert(source);
            first
        };
        if first {
            self.bus.publish(BusEvent::DetectionStarted { camera_id: camera.to_string(), kind, topic: topic.to_string(), at });
        }
        self.badge(camera);
    }

    /// `source` no longer sees `kind`.
    pub fn end(&self, camera: &str, kind: EventType, source: Source, at: DateTime<Utc>) {
        let last = {
            let mut active = self.active.lock().expect("detections lock");
            let Some(kinds) = active.get_mut(camera) else { return };
            let Some(sources) = kinds.get_mut(&kind) else { return };
            if !sources.remove(&source) {
                return;
            }
            let last = sources.is_empty();
            if last {
                kinds.remove(&kind);
            }
            last
        };
        if last {
            self.bus.publish(BusEvent::DetectionEnded { camera_id: camera.to_string(), kind, at });
        }
        self.badge(camera);
    }

    /// End everything `source` saw on `camera` (it stopped or lost contact).
    pub fn end_all(&self, camera: &str, source: Source, at: DateTime<Utc>) {
        let kinds: Vec<EventType> = {
            let active = self.active.lock().expect("detections lock");
            active.get(camera).map(|k| k.iter().filter(|(_, s)| s.contains(&source)).map(|(k, _)| *k).collect()).unwrap_or_default()
        };
        for kind in kinds {
            self.end(camera, kind, source, at);
        }
    }

    fn badge(&self, camera: &str) {
        let on = self.active.lock().expect("detections lock").get(camera).is_some_and(|k| !k.is_empty());
        self.live.update(camera, |l| l.motion_active = on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::CameraLive;

    fn setup() -> (Detections, tokio::sync::broadcast::Receiver<BusEvent>, Arc<LiveRegistry>) {
        let bus = Bus::new();
        let rx = bus.subscribe();
        let live = Arc::new(LiveRegistry::default());
        live.set("cam", CameraLive::connecting());
        (Detections::new(bus, live.clone()), rx, live)
    }

    fn starts_and_ends(rx: &mut tokio::sync::broadcast::Receiver<BusEvent>) -> Vec<&'static str> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() {
            out.push(match e {
                BusEvent::DetectionStarted { .. } => "start",
                BusEvent::DetectionEnded { .. } => "end",
                _ => "other",
            });
        }
        out
    }

    #[test]
    fn one_source_ending_never_cuts_what_the_other_still_sees() {
        let (d, mut rx, live) = setup();
        let now = Utc::now();
        d.start("cam", EventType::Motion, Source::Software, "sw", now);
        d.start("cam", EventType::Motion, Source::Camera, "onvif", now);
        assert_eq!(starts_and_ends(&mut rx), ["start"], "the second source joins silently");
        d.end("cam", EventType::Motion, Source::Software, now);
        assert!(starts_and_ends(&mut rx).is_empty(), "the camera still sees motion");
        assert!(live.get("cam").unwrap().motion_active);
        d.end("cam", EventType::Motion, Source::Camera, now);
        assert_eq!(starts_and_ends(&mut rx), ["end"]);
        assert!(!live.get("cam").unwrap().motion_active);
    }

    #[test]
    fn repeated_or_stray_reports_change_nothing() {
        let (d, mut rx, _) = setup();
        let now = Utc::now();
        d.end("cam", EventType::Motion, Source::Camera, now);
        d.start("cam", EventType::Person, Source::Camera, "p", now);
        d.start("cam", EventType::Person, Source::Camera, "p", now);
        d.end("cam", EventType::Motion, Source::Camera, now);
        assert_eq!(starts_and_ends(&mut rx), ["start"]);
        d.start("cam", EventType::Motion, Source::Software, "sw", now);
        d.end_all("cam", Source::Camera, now);
        assert_eq!(starts_and_ends(&mut rx), ["start", "end"], "person ended; software motion stays");
    }
}
