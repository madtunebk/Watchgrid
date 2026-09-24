//! Which transitions become notifications. Pure: no I/O.

use std::time::Duration;

use watchgrid_model::{EventType, NotificationLevel, NotificationSettings};

use crate::bus::BusEvent;

/// A notification about to be raised.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub kind: &'static str,
    pub camera_id: Option<String>,
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub link: Option<String>,
}

impl Draft {
    /// Minimum time between two notifications of this kind for one camera.
    pub fn cooldown(&self) -> Duration {
        let mins = match self.kind {
            "camera_offline" => 10,
            "person" | "vehicle" => 2,
            "storage_low" => 6 * 60,
            _ => 1,
        };
        Duration::from_secs(mins * 60)
    }
}

fn camera_draft(kind: &'static str, id: &str, level: NotificationLevel, title: String, message: String) -> Draft {
    Draft { kind, camera_id: Some(id.to_string()), level, title, message, link: Some(format!("/cameras/{id}")) }
}

/// The notification for a bus event, if the settings ask for one.
/// `name` is the camera's display name; `was_offline` whether an offline
/// notification went out for it (so "back online" pairs with it).
pub fn draft(event: &BusEvent, s: &NotificationSettings, name: &str, was_offline: bool) -> Option<Draft> {
    match event {
        BusEvent::CameraOffline { camera_id, reason, .. } if s.camera_offline => {
            Some(camera_draft("camera_offline", camera_id, NotificationLevel::Error, format!("{name} is offline"), reason.clone()))
        }
        BusEvent::CameraOnline { camera_id, .. } if s.camera_offline && was_offline => {
            Some(camera_draft("camera_online", camera_id, NotificationLevel::Success, format!("{name} is back online"), "Live view and recording work again.".into()))
        }
        BusEvent::DetectionStarted { camera_id, kind, .. } => {
            let (key, what) = match kind {
                EventType::Person if s.person_detected => ("person", "Person"),
                EventType::Vehicle if s.vehicle_detected => ("vehicle", "Vehicle"),
                _ => return None,
            };
            Some(Draft {
                kind: key,
                camera_id: Some(camera_id.clone()),
                level: NotificationLevel::Info,
                title: format!("{what} detected on {name}"),
                message: "Open the events list to watch it.".into(),
                link: Some(format!("/events?camera={camera_id}")),
            })
        }
        BusEvent::RecordingStopped { camera_id, recording_id, error: Some(error), .. } if s.recording_failed => {
            let (level, title) = match recording_id {
                Some(_) => (NotificationLevel::Warning, format!("Recording on {name} ended early")),
                None => (NotificationLevel::Error, format!("Recording on {name} failed")),
            };
            Some(camera_draft("recording_failed", camera_id, level, title, error.clone()))
        }
        _ => None,
    }
}

/// Low disk space, if the settings ask for it.
pub fn storage_low(s: &NotificationSettings, free: u64, threshold: u64) -> Option<Draft> {
    (s.storage_low && free < threshold).then(|| Draft {
        kind: "storage_low",
        camera_id: None,
        level: NotificationLevel::Warning,
        title: "Storage almost full".into(),
        message: format!("{:.1} GB free. Old recordings are removed by the retention policy, if one is set.", free as f64 / 1e9),
        link: Some("/storage".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn all_on() -> NotificationSettings {
        NotificationSettings { camera_offline: true, person_detected: true, vehicle_detected: true, storage_low: true, recording_failed: true, webhook_url: None }
    }

    #[test]
    fn follows_the_settings() {
        let offline = BusEvent::CameraOffline { camera_id: "c".into(), reason: "timeout".into(), at: Utc::now() };
        let d = draft(&offline, &all_on(), "Hallway", false).unwrap();
        assert_eq!((d.kind, d.title.as_str(), d.level), ("camera_offline", "Hallway is offline", NotificationLevel::Error));
        let off = NotificationSettings { camera_offline: false, ..all_on() };
        assert!(draft(&offline, &off, "Hallway", false).is_none());

        let online = BusEvent::CameraOnline { camera_id: "c".into(), at: Utc::now() };
        assert!(draft(&online, &all_on(), "Hallway", false).is_none(), "only after an offline notice");
        assert!(draft(&online, &all_on(), "Hallway", true).is_some());

        let motion = BusEvent::DetectionStarted { camera_id: "c".into(), kind: EventType::Motion, topic: "x".into(), at: Utc::now() };
        assert!(draft(&motion, &all_on(), "Hallway", false).is_none(), "plain motion is too noisy");
        let person = BusEvent::DetectionStarted { camera_id: "c".into(), kind: EventType::Person, topic: "x".into(), at: Utc::now() };
        assert_eq!(draft(&person, &all_on(), "Hallway", false).unwrap().kind, "person");

        let ok = BusEvent::RecordingStopped { camera_id: "c".into(), recording_id: Some("r".into()), error: None, at: Utc::now() };
        assert!(draft(&ok, &all_on(), "Hallway", false).is_none());
        let failed = BusEvent::RecordingStopped { camera_id: "c".into(), recording_id: None, error: Some("the disk is full".into()), at: Utc::now() };
        assert_eq!(draft(&failed, &all_on(), "Hallway", false).unwrap().level, NotificationLevel::Error);
    }

    #[test]
    fn storage_threshold() {
        assert!(storage_low(&all_on(), 10, 20).is_some());
        assert!(storage_low(&all_on(), 30, 20).is_none());
        assert!(storage_low(&NotificationSettings { storage_low: false, ..all_on() }, 10, 20).is_none());
    }
}
