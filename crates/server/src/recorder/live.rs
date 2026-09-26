//! Recordings still being written, so they can be listed and played before
//! they finish. A clip in progress is an incomplete MP4 (no `moov` yet);
//! [`LiveClip::snapshot`] describes a complete file made of what is on
//! disk so far plus an index built from the writer's in-memory tables.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use watchgrid_model::{Recording, RecordingReason};

use super::writer::Index;
use crate::media::mp4;
use crate::media::{TIMESCALE, VideoTrack};

pub struct LiveClip {
    pub recording_id: String,
    pub camera_id: String,
    pub reason: RecordingReason,
    pub path: PathBuf,
    /// File layout from the writer: where the `mdat` size field is and
    /// where samples begin.
    pub mdat_size_at: u64,
    pub data_start: u64,
    pub video: VideoTrack,
    pub index: Arc<Mutex<Index>>,
    /// Wall-clock time of the first frame (unset until one is written).
    pub started_at: Mutex<Option<DateTime<Utc>>>,
}

/// A complete MP4 for what is on disk: file bytes `[0, data_end)` with the
/// `mdat` size patched, then `moov`.
pub struct Snapshot {
    pub path: PathBuf,
    pub mdat_size_at: u64,
    pub mdat_size: u64,
    pub data_end: u64,
    pub moov: Vec<u8>,
}

impl Snapshot {
    pub fn len(&self) -> u64 {
        self.data_end + self.moov.len() as u64
    }
}

impl LiveClip {
    /// The recording as the API shows it, once it has started.
    pub fn recording(&self) -> Option<Recording> {
        let start_time = (*self.started_at.lock().expect("live clip lock"))?;
        let index = self.index.lock().expect("recording index lock");
        Some(Recording {
            id: self.recording_id.clone(),
            camera_id: self.camera_id.clone(),
            start_time,
            end_time: None,
            duration: (index.video.duration() / u64::from(TIMESCALE)) as u32,
            reason: self.reason,
            file_size: index.flushed,
            protected: false,
            event_ids: Vec::new(),
        })
    }

    /// What can be played right now (`None` before any video is on disk).
    pub fn snapshot(&self) -> Option<Snapshot> {
        let index = self.index.lock().expect("recording index lock");
        let video = index.video.within(index.flushed);
        if video.len() == 0 {
            return None;
        }
        let audio = index.audio.as_ref().map(|(track, table)| (track, table.within(index.flushed)));
        let data_end = index.flushed;
        let moov = mp4::moov(&self.video, &video, audio.as_ref().map(|(t, table)| (*t, table)));
        Some(Snapshot { path: self.path.clone(), mdat_size_at: self.mdat_size_at, mdat_size: mp4::mdat_size(data_end - self.data_start), data_end, moov })
    }
}

/// Clips in progress, by recording id.
#[derive(Default)]
pub struct LiveClips(Mutex<HashMap<String, Arc<LiveClip>>>);

impl LiveClips {
    pub fn insert(&self, clip: Arc<LiveClip>) {
        self.0.lock().expect("live clips lock").insert(clip.recording_id.clone(), clip);
    }

    pub fn remove(&self, recording_id: &str) {
        self.0.lock().expect("live clips lock").remove(recording_id);
    }

    pub fn get(&self, recording_id: &str) -> Option<Arc<LiveClip>> {
        self.0.lock().expect("live clips lock").get(recording_id).cloned()
    }

    pub fn all(&self) -> Vec<Arc<LiveClip>> {
        self.0.lock().expect("live clips lock").values().cloned().collect()
    }
}
