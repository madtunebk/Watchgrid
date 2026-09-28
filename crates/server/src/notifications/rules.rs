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
            "motion" => 5,
            "security" => 10,
            "storage_low" => 6 * 60,
            _ => 1,
        };
        Duration::from_secs(mins * 60)
    }
}

fn camera_draft(kind: &'static str, id: &str, level: NotificationLevel, title: String, message: String) -> Draft {
    Draft { kind, camera_id: Some(id.to_string()), level, title, message, link: Some(format!("/cameras/{id}")) }
}

/// The camera an event is about, as the rules need it.
pub struct CameraFacts<'a> {
    /// Display name.
    pub name: &'a str,
    /// Whether an offline notification went out for it (so "back online" pairs with it).
    pub was_offline: bool,
    /// Camera → Motion → "Notify on motion".
    pub notify_motion: bool,
}

/// The notification for a bus event, if the settings ask for one.
pub fn draft(event: &BusEvent, s: &NotificationSettings, camera: &CameraFacts) -> Option<Draft> {
    let (name, was_offline) = (camera.name, camera.was_offline);
    match event {
        BusEvent::CameraOffline { camera_id, reason, .. } if s.camera_offline => {
            Some(camera_draft("camera_offline", camera_id, NotificationLevel::Error, format!("{name} is offline"), reason.clone()))
        }
        BusEvent::CameraOnline { camera_id, .. } if s.camera_offline && was_offline => {
            Some(camera_draft("camera_online", camera_id, NotificationLevel::Success, format!("{name} is back online"), "Live view and recording work again.".into()))
        }
        BusEvent::SecurityAlert { camera_id, .. } if s.camera_security => Some(Draft {
            kind: "security",
            camera_id: Some(camera_id.clone()),
            level: NotificationLevel::Warning,
            title: format!("Failed sign-in on {name}"),
            message: "The camera reported a sign-in with a wrong password. If it wasn't you, change the camera's password.".into(),
            link: Some(format!("/events?camera={camera_id}&types=security")),
        }),
        BusEvent::DetectionStarted { camera_id, kind, .. } => {
            let (key, what) = match kind {
                EventType::Person if s.person_detected => ("person", "Person"),
                EventType::Vehicle if s.vehicle_detected => ("vehicle", "Vehicle"),
                EventType::Motion if camera.notify_motion => ("motion", "Motion"),
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

/// Cameras that went offline (or came back) within a few seconds of each
/// other — a switch or power cut — as one notification instead of one per
/// camera. `items`: each camera's name and its own notification.
pub fn burst(kind: &'static str, items: &[(String, Draft)]) -> Draft {
    let n = items.len();
    let offline = kind == "camera_offline";
    let names: Vec<&str> = items.iter().map(|(name, _)| name.as_str()).collect();
    let message = if offline {
        // Grouped by reason: "Hall, Yard: connection refused; Gate: timed out".
        let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
        for (name, d) in items {
            match groups.iter_mut().find(|(why, _)| *why == d.message) {
                Some((_, names)) => names.push(name),
                None => groups.push((&d.message, vec![name])),
            }
        }
        groups.iter().map(|(why, names)| format!("{}: {why}", some(names))).collect::<Vec<_>>().join("; ")
    } else {
        format!("{}. Live view and recording work again.", some(&names))
    };
    Draft {
        kind,
        camera_id: None,
        level: items[0].1.level,
        title: if offline { format!("{n} cameras are offline") } else { format!("{n} cameras are back online") },
        message,
        link: Some(if offline { "/cameras?status=offline".into() } else { "/cameras".into() }),
    }
}

/// Up to ten names, then how many more.
fn some(names: &[&str]) -> String {
    const SHOWN: usize = 10;
    match names.len() {
        0..=SHOWN => names.join(", "),
        n => format!("{} and {} more", names[..SHOWN].join(", "), n - SHOWN),
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

    fn cam(name: &str, was_offline: bool) -> CameraFacts<'_> {
        CameraFacts { name, was_offline, notify_motion: false }
    }
    use chrono::Utc;

    fn all_on() -> NotificationSettings {
        NotificationSettings {
            camera_offline: true,
            person_detected: true,
            vehicle_detected: true,
            storage_low: true,
            recording_failed: true,
            camera_security: true,
            webhook_url: None,
        }
    }

    #[test]
    fn follows_the_settings() {
        let offline = BusEvent::CameraOffline { camera_id: "c".into(), reason: "timeout".into(), at: Utc::now() };
        let d = draft(&offline, &all_on(), &cam("Hallway", false)).unwrap();
        assert_eq!((d.kind, d.title.as_str(), d.level), ("camera_offline", "Hallway is offline", NotificationLevel::Error));
        let off = NotificationSettings { camera_offline: false, ..all_on() };
        assert!(draft(&offline, &off, &cam("Hallway", false)).is_none());

        let online = BusEvent::CameraOnline { camera_id: "c".into(), at: Utc::now() };
        assert!(draft(&online, &all_on(), &cam("Hallway", false)).is_none(), "only after an offline notice");
        assert!(draft(&online, &all_on(), &cam("Hallway", true)).is_some());

        let motion = BusEvent::DetectionStarted { camera_id: "c".into(), kind: EventType::Motion, topic: "x".into(), at: Utc::now() };
        assert!(draft(&motion, &all_on(), &cam("Hallway", false)).is_none(), "plain motion is too noisy");
        let chosen = CameraFacts { notify_motion: true, ..cam("Hallway", false) };
        let d = draft(&motion, &all_on(), &chosen).expect("chosen for this camera");
        assert_eq!((d.kind, d.title.as_str(), d.cooldown().as_secs()), ("motion", "Motion detected on Hallway", 300));
        let person = BusEvent::DetectionStarted { camera_id: "c".into(), kind: EventType::Person, topic: "x".into(), at: Utc::now() };
        assert_eq!(draft(&person, &all_on(), &cam("Hallway", false)).unwrap().kind, "person");

        let ok = BusEvent::RecordingStopped { camera_id: "c".into(), recording_id: Some("r".into()), error: None, at: Utc::now() };
        assert!(draft(&ok, &all_on(), &cam("Hallway", false)).is_none());
        let failed = BusEvent::RecordingStopped { camera_id: "c".into(), recording_id: None, error: Some("the disk is full".into()), at: Utc::now() };
        assert_eq!(draft(&failed, &all_on(), &cam("Hallway", false)).unwrap().level, NotificationLevel::Error);
    }

    #[test]
    fn storage_threshold() {
        assert!(storage_low(&all_on(), 10, 20).is_some());
        assert!(storage_low(&all_on(), 30, 20).is_none());
        assert!(storage_low(&NotificationSettings { storage_low: false, ..all_on() }, 10, 20).is_none());
    }

    #[test]
    fn failed_camera_sign_ins_warn_unless_switched_off() {
        let alert = BusEvent::SecurityAlert { camera_id: "cam-a".into(), topic: "UserAlarm/IllegalAccess".into(), at: chrono::Utc::now() };
        let d = draft(&alert, &all_on(), &cam("Hall", false)).expect("notified");
        assert_eq!((d.kind, d.level, d.title.as_str()), ("security", NotificationLevel::Warning, "Failed sign-in on Hall"));
        assert_eq!(d.cooldown(), Duration::from_secs(600));
        assert!(draft(&alert, &NotificationSettings { camera_security: false, ..all_on() }, &cam("Hall", false)).is_none());
    }

    #[test]
    fn a_burst_of_outages_is_one_notification() {
        let off = |name: &str, why: &str| (name.to_string(), camera_draft("camera_offline", "x", NotificationLevel::Error, format!("{name} is offline"), why.into()));
        let d = burst("camera_offline", &[off("Hall", "connection refused"), off("Gate", "timed out"), off("Yard", "connection refused")]);
        assert_eq!(d.title, "3 cameras are offline");
        assert_eq!(d.message, "Hall, Yard: connection refused; Gate: timed out");
        assert_eq!((d.camera_id, d.level, d.link.as_deref()), (None, NotificationLevel::Error, Some("/cameras?status=offline")));

        let on: Vec<(String, Draft)> = (1..=12).map(|i| (format!("cam{i}"), camera_draft("camera_online", "x", NotificationLevel::Success, String::new(), String::new()))).collect();
        let d = burst("camera_online", &on);
        assert_eq!(d.title, "12 cameras are back online");
        assert!(d.message.starts_with("cam1, cam2") && d.message.contains("and 2 more"), "{}", d.message);
    }
}
