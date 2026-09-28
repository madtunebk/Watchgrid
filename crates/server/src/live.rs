//! Live camera state — memory only, never written to PostgreSQL.
//! The supervisor writes it; API reads overlay it onto stored config.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use watchgrid_model::{Camera, CameraStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct CameraLive {
    pub status: CameraStatus,
    pub connected_since: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    /// The camera reports motion right now (ONVIF).
    pub motion_active: bool,
    /// ONVIF events don't work: software detection stands in.
    pub motion_fallback: bool,
    /// Watchgrid's own detection, while it runs.
    pub software_motion: Option<watchgrid_model::SoftwareMotionStatus>,
}

impl CameraLive {
    pub fn connecting() -> Self {
        Self {
            status: CameraStatus::Connecting,
            connected_since: None,
            last_error: None,
            motion_active: false,
            motion_fallback: false,
            software_motion: None,
        }
    }
}

#[derive(Default)]
pub struct LiveRegistry(RwLock<HashMap<String, CameraLive>>);

impl LiveRegistry {
    pub fn set(&self, id: &str, live: CameraLive) {
        self.0.write().expect("live registry poisoned").insert(id.to_string(), live);
    }

    pub fn update(&self, id: &str, f: impl FnOnce(&mut CameraLive)) {
        if let Some(live) = self.0.write().expect("live registry poisoned").get_mut(id) {
            f(live);
        }
    }

    pub fn get(&self, id: &str) -> Option<CameraLive> {
        self.0.read().expect("live registry poisoned").get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.0.write().expect("live registry poisoned").remove(id);
    }

    /// Put the live state onto a camera loaded from the database.
    pub fn overlay(&self, camera: &mut Camera) {
        let Some(live) = self.get(&camera.id) else { return };
        camera.status = live.status;
        camera.connected_since = live.connected_since;
        camera.motion_active = live.motion_active;
        camera.motion_fallback = live.motion_fallback;
        camera.software_motion = live.software_motion.clone();
        // The streams' own states and facts come from the media hub.
    }
}
