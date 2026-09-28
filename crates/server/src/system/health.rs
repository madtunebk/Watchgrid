//! What "the NVR is fine" means, part by part. Cheap enough for the 5 s
//! status poll: no file writes, no scans of the recordings.

use std::path::Path;

use watchgrid_model::{Camera, CameraStatus, CheckLevel, DetectorState, HealthCheck};

/// Disk use from which the recordings folder is reported as nearly full.
const DISK_WARNING: f32 = 95.0;

fn check(name: &str, level: CheckLevel, detail: impl Into<String>) -> HealthCheck {
    HealthCheck { name: name.into(), level, detail: detail.into() }
}

/// The recordings folder: can new recordings be written, and is there room.
pub fn recordings(root: &Path, disk_usage: Option<f32>) -> HealthCheck {
    const NAME: &str = "Recordings folder";
    if !crate::storage::disk::writable(root) {
        return check(NAME, CheckLevel::Error, "New recordings can't be written (missing or read-only)");
    }
    match disk_usage {
        Some(p) if p >= DISK_WARNING => check(NAME, CheckLevel::Warning, format!("Disk {p:.0}% full")),
        Some(p) => check(NAME, CheckLevel::Ok, format!("Writable, disk {p:.0}% used")),
        None => check(NAME, CheckLevel::Ok, "Writable"),
    }
}

/// Enabled cameras that should stream but don't. Connecting is not a problem yet.
pub fn cameras(cams: &[Camera]) -> HealthCheck {
    const NAME: &str = "Cameras";
    let enabled: Vec<&Camera> = cams.iter().filter(|c| c.enabled).collect();
    let down: Vec<&str> = enabled.iter().filter(|c| matches!(c.status, CameraStatus::Offline | CameraStatus::Error)).map(|c| c.name.as_str()).collect();
    match down.len() {
        0 if enabled.is_empty() => check(NAME, CheckLevel::Ok, "None enabled"),
        0 => check(NAME, CheckLevel::Ok, format!("{} online", enabled.len())),
        1 => check(NAME, CheckLevel::Warning, format!("{} offline", down[0])),
        n if n <= 3 => check(NAME, CheckLevel::Warning, format!("{} offline", down.join(", "))),
        n => check(NAME, CheckLevel::Warning, format!("{n} of {} offline", enabled.len())),
    }
}

/// Motion detection per camera: camera events that fail (software detection
/// stands in), and Watchgrid's own detection when it can't work (no video,
/// wrong codec). `None` when no enabled online camera detects motion.
pub fn motion(cams: &[Camera]) -> Option<HealthCheck> {
    const NAME: &str = "Motion detection";
    // An offline camera is reported by the Cameras check.
    let detecting: Vec<&Camera> = cams.iter().filter(|c| c.enabled && c.motion.enabled && c.status == CameraStatus::Online).collect();
    if detecting.is_empty() {
        return None;
    }
    let failing = |c: &&&Camera| c.software_motion.as_ref().is_some_and(|s| s.state == DetectorState::Failing);
    let why = |c: &Camera| c.software_motion.as_ref().and_then(|s| s.detail.clone()).unwrap_or_default();
    let blind: Vec<(&str, String)> = detecting.iter().filter(failing).map(|c| (c.name.as_str(), why(c))).collect();
    if !blind.is_empty() {
        return Some(check(NAME, CheckLevel::Error, format!("Not detecting on {}", by_reason(&blind))));
    }
    let standing_in: Vec<&str> = detecting.iter().filter(|c| c.motion_fallback).map(|c| c.name.as_str()).collect();
    Some(match standing_in.as_slice() {
        [] => check(NAME, CheckLevel::Ok, format!("Working on {}", detecting.len())),
        names => check(NAME, CheckLevel::Warning, format!("{}: camera events fail, Watchgrid detects motion itself", some_names(names))),
    })
}

/// Cameras with the same problem said once: "Hall, Yard: why; Gate: other".
fn by_reason(items: &[(&str, String)]) -> String {
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for (name, why) in items {
        match groups.iter_mut().find(|(w, _)| *w == why.as_str()) {
            Some((_, names)) => names.push(name),
            None => groups.push((why.as_str(), vec![name])),
        }
    }
    groups.iter().map(|(why, names)| if why.is_empty() { some_names(names) } else { format!("{}: {why}", some_names(names)) }).collect::<Vec<_>>().join("; ")
}

/// Up to three names; beyond that the count and the first three.
fn some_names(names: &[&str]) -> String {
    match names.len() {
        0..=3 => names.join(", "),
        n => format!("{n} cameras ({} and {} more)", names[..3].join(", "), n - 3),
    }
}

/// Destinations that can't take uploads (rejected credentials…); `None`
/// when there are none configured.
pub fn exports(names_with_problems: &[String], configured: usize) -> Option<HealthCheck> {
    const NAME: &str = "Export destinations";
    (configured > 0).then(|| match names_with_problems {
        [] => check(NAME, CheckLevel::Ok, format!("{configured} ready")),
        [one] => check(NAME, CheckLevel::Warning, format!("{one} needs attention")),
        many => check(NAME, CheckLevel::Warning, format!("{} need attention", many.len())),
    })
}

pub fn database() -> HealthCheck {
    check("Database", CheckLevel::Ok, "Answering")
}

#[cfg(test)]
mod tests {
    use watchgrid_model::ServerHealth;

    use super::*;

    #[test]
    fn the_worst_check_decides_and_messages_are_plain() {
        let ok = database();
        let dir = std::env::temp_dir();
        assert_eq!(recordings(&dir, Some(41.0)).detail, "Writable, disk 41% used");
        assert_eq!(recordings(&dir, Some(97.0)).level, CheckLevel::Warning);
        let gone = recordings(Path::new("/nonexistent/watchgrid"), Some(10.0));
        assert_eq!(gone.level, CheckLevel::Error);

        assert_eq!(ServerHealth::of(&[ok.clone()]), ServerHealth::Running);
        assert_eq!(ServerHealth::of(&[ok.clone(), recordings(&dir, Some(97.0))]), ServerHealth::Degraded);
        assert_eq!(ServerHealth::of(&[ok, recordings(&dir, Some(97.0)), gone]), ServerHealth::Stopped);

        assert!(exports(&[], 0).is_none(), "no destinations: nothing to report");

        let why = |s: &str| s.to_string();
        let forty: Vec<(&str, String)> = ["c1", "c2", "c3", "c4", "c5"].iter().map(|n| (*n, why("can't decode"))).collect();
        assert_eq!(by_reason(&forty), "5 cameras (c1, c2, c3 and 2 more): can't decode", "one reason said once");
        assert_eq!(by_reason(&[("Hall", why("no video")), ("Gate", why("can't decode")), ("Yard", why("no video"))]), "Hall, Yard: no video; Gate: can't decode");
        assert_eq!(exports(&["MinIO".into()], 2).unwrap().detail, "MinIO needs attention");
    }
}
