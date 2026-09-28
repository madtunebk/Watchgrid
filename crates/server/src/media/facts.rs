//! What each camera stream was last seen to be (codec, size, frame rate,
//! bitrate), kept after its feed closes. The main stream is only open while
//! something uses it; the camera page and the capacity estimate still need
//! to know what it delivers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::TrackInfo;
use super::hub::Key;

#[derive(Clone)]
pub struct StreamFacts {
    pub info: Arc<TrackInfo>,
    pub fps: Option<f32>,
    /// kbit/s over the last measuring window.
    pub kbps: Option<u32>,
}

#[derive(Default)]
pub struct FactsCache(Mutex<HashMap<Key, StreamFacts>>);

impl FactsCache {
    /// The stream (re)started with these parameters.
    pub fn info(&self, key: &Key, info: Arc<TrackInfo>) {
        let mut all = self.0.lock().expect("facts lock");
        match all.get_mut(key) {
            Some(f) => f.info = info,
            None => {
                all.insert(key.clone(), StreamFacts { info, fps: None, kbps: None });
            }
        }
    }

    /// A measuring window ended.
    pub fn rate(&self, key: &Key, fps: f32, kbps: u32) {
        if let Some(f) = self.0.lock().expect("facts lock").get_mut(key) {
            f.fps = Some(fps);
            f.kbps = Some(kbps);
        }
    }

    pub fn get(&self, key: &Key) -> Option<StreamFacts> {
        self.0.lock().expect("facts lock").get(key).cloned()
    }

    /// The camera is gone (or its streams changed): forget what it was.
    pub fn forget(&self, camera_id: &str) {
        self.0.lock().expect("facts lock").retain(|(id, _), _| id != camera_id);
    }
}
