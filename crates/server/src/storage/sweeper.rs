//! Applies the retention policy: periodically, and right after the policy
//! changes.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::Notify;

use super::plan::{self, Usage};
use super::{disk, retention};
use crate::bus::{Bus, BusEvent};
use crate::recordings::{self, RecordingFiles};

const INTERVAL: Duration = Duration::from_secs(600);
/// Event history kept when no age limit is set.
const DEFAULT_EVENT_DAYS: u32 = 365;

pub struct Sweeper {
    db: PgPool,
    files: Arc<RecordingFiles>,
    bus: Bus,
    wake: Notify,
}

impl Sweeper {
    pub fn new(db: PgPool, files: Arc<RecordingFiles>, bus: Bus) -> Self {
        Self { db, files, bus, wake: Notify::new() }
    }

    /// Run a pass soon (e.g. after the policy changed).
    pub fn kick(&self) {
        self.wake.notify_one();
    }

    /// The background loop; passes never overlap.
    pub async fn run(self: Arc<Self>) {
        loop {
            match self.pass().await {
                Ok(0) => tracing::debug!("retention pass: nothing to delete"),
                Ok(n) => {
                    tracing::info!("retention pass: deleted {n} recording(s)");
                    self.bus.publish(BusEvent::RecordingsChanged);
                }
                Err(e) => tracing::warn!("retention pass failed: {e}"),
            }
            tokio::select! {
                _ = tokio::time::sleep(INTERVAL) => {}
                _ = self.wake.notified() => {}
            }
        }
    }

    pub(super) async fn pass(&self) -> Result<usize, String> {
        let policy = retention::load(&self.db).await.map_err(|e| format!("{e:?}"))?;
        // Event history follows the recordings' age limit (a year without one).
        let event_days = policy.max_age_days.unwrap_or(DEFAULT_EVENT_DAYS);
        match crate::events::purge_events_older_than(&self.db, event_days).await {
            Ok(0) => {}
            Ok(n) => {
                tracing::info!("retention pass: deleted {n} event(s) older than {event_days} days");
                self.bus.publish(BusEvent::EventsChanged);
            }
            Err(e) => tracing::warn!("event retention failed: {e}"),
        }
        match crate::thumbs::prune(&self.db, &self.files).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("retention pass: removed {n} unused thumbnail(s)"),
            Err(e) => tracing::warn!("thumbnail cleanup failed: {e}"),
        }
        let mut candidates = recordings::retention_candidates(&self.db).await.map_err(|e| e.to_string())?;
        if policy == retention::default_policy() && candidates.iter().all(|c| c.camera_max_days.is_none()) {
            return Ok(0);
        }
        let recordings_bytes = recordings::usage_by_camera(&self.db).await.map_err(|e| e.to_string())?.iter().map(|u| u.bytes).sum();
        let free = self.volumes(&mut candidates);
        let doomed = plan::plan(&policy, &candidates, Usage { recordings_bytes, free }, Utc::now());

        let mut deleted = 0;
        for id in doomed {
            match self.delete(&id).await {
                Ok(()) => deleted += 1,
                Err(e) => tracing::warn!(recording = %id, "retention could not delete: {e}"),
            }
        }
        Ok(deleted)
    }

    /// Tag each candidate with the filesystem it is on; returns the free
    /// space of every such volume (and of the current recordings folder).
    fn volumes(&self, candidates: &mut [plan::Candidate]) -> Vec<(u64, u64)> {
        let default = self.files.root();
        let mut devices: HashMap<Option<String>, Option<u64>> = HashMap::new();
        let mut device_of = |root: &Option<String>| {
            *devices.entry(root.clone()).or_insert_with(|| {
                let dir = root.as_ref().map_or_else(|| default.clone(), PathBuf::from);
                std::fs::metadata(dir).ok().map(|m| m.dev())
            })
        };
        let mut free: Vec<(u64, u64)> = Vec::new();
        let add = |volume: Option<u64>, dir: PathBuf, free: &mut Vec<(u64, u64)>| {
            if let Some(v) = volume.filter(|v| !free.iter().any(|(seen, _)| seen == v)) {
                if let Ok(d) = disk::space(&dir) {
                    free.push((v, d.free));
                }
            }
        };
        add(device_of(&None), default.clone(), &mut free);
        for c in candidates.iter_mut() {
            c.volume = device_of(&c.root);
            let dir = c.root.as_ref().map_or_else(|| default.clone(), PathBuf::from);
            add(c.volume, dir, &mut free);
        }
        free
    }

    async fn delete(&self, id: &str) -> Result<(), String> {
        recordings::delete_recording(&self.db, &self.files, id).await.map_err(|e| e.to_string())?;
        tracing::info!(recording = %id, "deleted by retention");
        Ok(())
    }
}
