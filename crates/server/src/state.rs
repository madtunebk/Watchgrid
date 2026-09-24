use std::path::PathBuf;
use std::sync::Arc;

use sqlx::PgPool;

use crate::bus::Bus;
use crate::credentials::CredentialStore;
use crate::live::LiveRegistry;
use crate::media::MediaHub;
use crate::recorder::{self, Recorder};
use crate::recordings::RecordingFiles;
use crate::auth::LoginLimiter;
use crate::storage::Sweeper;
use crate::system::Sampler;
use crate::system::logs::LogBuffer;
use crate::supervisor::{Deps, Supervisor};

/// Shared by all request handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub credentials: Arc<CredentialStore>,
    /// Live camera state (memory only).
    pub live: Arc<LiveRegistry>,
    pub bus: Bus,
    pub supervisor: Arc<Supervisor>,
    /// On-demand live video feeds.
    pub media: Arc<MediaHub>,
    pub recorder: Arc<Recorder>,
    pub recording_files: Arc<RecordingFiles>,
    /// Retention enforcement (started by `serve`).
    pub retention: Arc<Sweeper>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub metrics: Sampler,
    pub logs: LogBuffer,
    /// Address the server listens on (settings defaults).
    pub bind: std::net::SocketAddr,
    pub login_limiter: Arc<LoginLimiter>,
}

impl AppState {
    pub fn new(db: PgPool, credentials: CredentialStore, recordings_dir: PathBuf) -> Self {
        let deps = Deps { db: db.clone(), credentials: Arc::new(credentials), live: Arc::new(LiveRegistry::default()), bus: Bus::new() };
        Self::with_supervisor(Supervisor::new(deps.clone()), deps, recordings_dir)
    }

    /// State whose supervisor never connects to cameras (tests).
    #[cfg(test)]
    pub fn inert(db: PgPool, credentials: CredentialStore) -> Self {
        let deps = Deps { db, credentials: Arc::new(credentials), live: Arc::new(LiveRegistry::default()), bus: Bus::new() };
        Self::with_supervisor(Supervisor::inert(deps.clone()), deps, std::env::temp_dir().join("watchgrid-test-recordings"))
    }

    fn with_supervisor(supervisor: Supervisor, deps: Deps, recordings_dir: PathBuf) -> Self {
        let media = Arc::new(MediaHub::new(deps.db.clone(), deps.credentials.clone()));
        let files = Arc::new(RecordingFiles::new(recordings_dir));
        let recorder = Recorder::new(recorder::Deps { db: deps.db.clone(), hub: media.clone(), files: files.clone(), bus: deps.bus.clone() });
        let retention = Arc::new(Sweeper::new(deps.db.clone(), files.clone(), deps.bus.clone()));
        Self {
            db: deps.db,
            credentials: deps.credentials,
            live: deps.live,
            bus: deps.bus,
            supervisor: Arc::new(supervisor),
            media,
            retention,
            started_at: chrono::Utc::now(),
            metrics: Sampler::default(),
            logs: LogBuffer::default(),
            bind: "127.0.0.1:8090".parse().expect("valid default address"),
            login_limiter: Arc::default(),
            recorder: Arc::new(recorder),
            recording_files: files,
        }
    }
}
