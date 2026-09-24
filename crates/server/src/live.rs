//! Live camera state — memory only, never written to PostgreSQL.
//! The supervisor writes it; API reads overlay it onto stored config.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use watchgrid_model::{Camera, CameraStatus, StreamStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct CameraLive {
    pub status: CameraStatus,
    pub connected_since: Option<DateTime<Utc>>,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    /// kbit/s over the last measuring window.
    pub bitrate: Option<u32>,
    pub audio_codec: Option<String>,
    pub last_error: Option<String>,
    /// The camera reports motion right now (ONVIF).
    pub motion_active: bool,
}

impl CameraLive {
    pub fn connecting() -> Self {
        Self {
            status: CameraStatus::Connecting,
            connected_since: None,
            codec: None,
            width: None,
            height: None,
            fps: None,
            bitrate: None,
            audio_codec: None,
            last_error: None,
            motion_active: false,
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
        let s = &mut camera.main_stream;
        s.status = match live.status {
            CameraStatus::Online => StreamStatus::Active,
            CameraStatus::Error => StreamStatus::Error,
            _ => StreamStatus::Idle,
        };
        s.codec = live.codec;
        s.width = live.width;
        s.height = live.height;
        s.fps = live.fps;
        s.bitrate = live.bitrate;
        s.audio_codec = live.audio_codec;
    }
}
