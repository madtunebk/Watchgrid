//! Pure filtering of the camera list (search text + status).

use crate::api::{Camera, CameraStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Online,
    Offline,
    Recording,
    Disabled,
}

impl StatusFilter {
    fn matches(self, c: &Camera) -> bool {
        match self {
            Self::All => true,
            Self::Online => c.enabled && c.status == CameraStatus::Online,
            Self::Offline => c.enabled && c.status != CameraStatus::Online,
            Self::Recording => c.recording_active,
            Self::Disabled => !c.enabled,
        }
    }
}

pub fn apply(cameras: Vec<Camera>, search: &str, status: StatusFilter) -> Vec<Camera> {
    let needle = search.trim().to_lowercase();
    cameras
        .into_iter()
        .filter(|c| status.matches(c))
        .filter(|c| {
            needle.is_empty()
                || c.name.to_lowercase().contains(&needle)
                || c.host.to_lowercase().contains(&needle)
                || c.location.to_lowercase().contains(&needle)
        })
        .collect()
}
