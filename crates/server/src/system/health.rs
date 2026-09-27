//! What "the NVR is fine" means, part by part. Cheap enough for the 5 s
//! status poll: no file writes, no scans of the recordings.

use std::path::Path;

use watchgrid_model::{Camera, CameraStatus, CheckLevel, HealthCheck};

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
        assert_eq!(exports(&["MinIO".into()], 2).unwrap().detail, "MinIO needs attention");
    }
}
