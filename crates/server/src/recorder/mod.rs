//! Manual recording: one job per camera that copies the main stream's
//! H.264 frames (no decode, no re-encode) from the shared media hub into an
//! MP4 file.
//!
//! Lifecycle: IDLE → STARTING → RECORDING → FINALIZING → IDLE. Every exit
//! path — STOP, camera disconnect, write error, server shutdown — goes
//! through FINALIZING and ends in IDLE, so a camera can't stay stuck in REC.

mod job;
mod writer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::watch;
use watchgrid_model::{Camera, RecordingReason};

use crate::bus::Bus;
use crate::media::MediaHub;
use crate::recordings::RecordingFiles;

/// How long STOP waits for the file to be finalized.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// Waiting for the stream and its first keyframe.
    Starting,
    Recording,
    /// Writing the index and publishing the file.
    Finalizing,
}

/// What recording jobs need; cheap to clone.
#[derive(Clone)]
pub struct Deps {
    pub db: PgPool,
    pub hub: Arc<MediaHub>,
    pub files: Arc<RecordingFiles>,
    pub bus: Bus,
}

struct Job {
    phase: watch::Receiver<Phase>,
    stop: watch::Sender<bool>,
}

impl Job {
    fn running(&self) -> bool {
        *self.phase.borrow() != Phase::Idle
    }
}

pub struct Recorder {
    deps: Deps,
    jobs: Mutex<HashMap<String, Job>>,
}

impl Recorder {
    pub fn new(deps: Deps) -> Self {
        Self { deps, jobs: Mutex::new(HashMap::new()) }
    }

    /// Start a manual recording. Does nothing if one is already running.
    pub fn start(&self, camera_id: &str) {
        let mut jobs = self.jobs.lock().expect("recorder lock");
        if jobs.get(camera_id).is_some_and(Job::running) {
            return;
        }
        let (phase_tx, phase_rx) = watch::channel(Phase::Starting);
        let (stop_tx, stop_rx) = watch::channel(false);
        tokio::spawn(job::run(self.deps.clone(), camera_id.to_string(), stop_rx, phase_tx));
        jobs.insert(camera_id.to_string(), Job { phase: phase_rx, stop: stop_tx });
    }

    /// Stop a recording and wait until it is finalized (bounded).
    pub async fn stop(&self, camera_id: &str) {
        let phase = {
            let jobs = self.jobs.lock().expect("recorder lock");
            let Some(job) = jobs.get(camera_id) else { return };
            job.stop.send_replace(true);
            job.phase.clone()
        };
        wait_idle(phase).await;
    }

    pub fn phase(&self, camera_id: &str) -> Phase {
        self.jobs.lock().expect("recorder lock").get(camera_id).map_or(Phase::Idle, |j| *j.phase.borrow())
    }

    /// Put the recording state onto a camera.
    pub fn overlay(&self, camera: &mut Camera) {
        if self.phase(&camera.id) != Phase::Idle {
            camera.recording_active = true;
            camera.recording_reason = Some(RecordingReason::Manual);
        }
    }

    /// Finalize every running recording (server shutdown).
    pub async fn shutdown(&self) {
        let phases: Vec<_> = {
            let jobs = self.jobs.lock().expect("recorder lock");
            jobs.values().filter(|j| j.running()).map(|j| {
                j.stop.send_replace(true);
                j.phase.clone()
            }).collect()
        };
        if !phases.is_empty() {
            tracing::info!("finalizing {} recording(s) before shutdown", phases.len());
        }
        futures::future::join_all(phases.into_iter().map(wait_idle)).await;
    }
}

async fn wait_idle(mut phase: watch::Receiver<Phase>) {
    // The job ends in Idle; a dropped sender means the job is gone too.
    let done = phase.wait_for(|p| *p == Phase::Idle);
    if tokio::time::timeout(STOP_TIMEOUT, done).await.is_err() {
        tracing::error!("recording did not finalize within {} s", STOP_TIMEOUT.as_secs());
    }
}
