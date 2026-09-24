//! Applies the retention policy: periodically, and right after the policy
//! changes.

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
        if policy == retention::default_policy() {
            return Ok(0);
        }
        let candidates = recordings::retention_candidates(&self.db).await.map_err(|e| e.to_string())?;
        let recordings_bytes = recordings::usage_by_camera(&self.db).await.map_err(|e| e.to_string())?.iter().map(|u| u.bytes).sum();
        let free = disk::space(&self.files.root()).ok().map(|d| d.free);
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

    async fn delete(&self, id: &str) -> Result<(), String> {
        recordings::delete_recording(&self.db, &self.files, id).await?;
        tracing::info!(recording = %id, "deleted by retention");
        Ok(())
    }
}
