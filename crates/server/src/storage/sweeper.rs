//! Applies the retention policy: periodically, and right after the policy
//! changes. Deletes the file first, then its row, so an interrupted pass
//! never leaves a listed recording without a file for long (a missing file
//! is treated as already deleted on the next pass).

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
                    self.bus.publish(BusEvent::RecordingsDeleted);
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
        if policy == retention::default_policy() {
            return Ok(0);
        }
        let candidates = recordings::retention_candidates(&self.db).await.map_err(|e| e.to_string())?;
        let recordings_bytes = recordings::usage_by_camera(&self.db).await.map_err(|e| e.to_string())?.iter().map(|u| u.bytes).sum();
        let free = disk::space(self.files.root()).ok().map(|d| d.free);
        let doomed = plan::plan(&policy, &candidates.iter().map(|c| c.candidate.clone()).collect::<Vec<_>>(), Usage { recordings_bytes, free }, Utc::now());

        let mut deleted = 0;
        for id in doomed {
            let Some(c) = candidates.iter().find(|c| c.candidate.id == id) else { continue };
            match self.delete(&id, &c.path).await {
                Ok(()) => deleted += 1,
                Err(e) => tracing::warn!(recording = %id, "retention could not delete: {e}"),
            }
        }
        Ok(deleted)
    }

    async fn delete(&self, id: &str, relative: &str) -> Result<(), String> {
        let path = self.files.resolve(relative).ok_or("unsafe path")?;
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
        recordings::delete(&self.db, id).await.map_err(|e| e.to_string())?;
        tracing::info!(recording = %id, "deleted by retention");
        Ok(())
    }
}
