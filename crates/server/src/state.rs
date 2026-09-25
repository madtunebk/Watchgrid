use std::path::PathBuf;
use std::sync::Arc;

use sqlx::PgPool;

use crate::bus::Bus;
use crate::credentials::CredentialStore;
use crate::live::LiveRegistry;
use crate::exports::Exports;
use crate::media::MediaHub;
use crate::onvif::{WatchDeps, Watchers};
use crate::recorder::{self, AutoRecorders, Recorder};
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
    /// ONVIF event watchers (motion from cameras).
    pub onvif: Arc<Watchers>,
    /// On-demand live video feeds.
    pub media: Arc<MediaHub>,
    pub recorder: Arc<Recorder>,
    /// Event recording controllers (cameras in "events" mode).
    pub auto_record: Arc<AutoRecorders>,
    pub recording_files: Arc<RecordingFiles>,
    /// Upload queue for export destinations.
    pub exports: Arc<Exports>,
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
        let credentials = Arc::new(credentials);
        let hub = Arc::new(MediaHub::new(db.clone(), credentials.clone()));
        let deps = Deps { db: db.clone(), credentials, live: Arc::new(LiveRegistry::default()), bus: Bus::new(), hub };
        let watch = WatchDeps { db: deps.db.clone(), credentials: deps.credentials.clone(), live: deps.live.clone(), bus: deps.bus.clone() };
        Self::with_supervisor(Supervisor::new(deps.clone()), Watchers::new(watch), deps, recordings_dir, true)
    }

    /// State whose supervisor never connects to cameras (tests).
    #[cfg(test)]
    pub fn inert(db: PgPool, credentials: CredentialStore) -> Self {
        let credentials = Arc::new(credentials);
        let hub = Arc::new(MediaHub::new(db.clone(), credentials.clone()));
        let deps = Deps { db, credentials, live: Arc::new(LiveRegistry::default()), bus: Bus::new(), hub };
        let watch = WatchDeps { db: deps.db.clone(), credentials: deps.credentials.clone(), live: deps.live.clone(), bus: deps.bus.clone() };
        Self::with_supervisor(Supervisor::inert(deps.clone()), Watchers::inert(watch), deps, std::env::temp_dir().join("watchgrid-test-recordings"), false)
    }

    fn with_supervisor(supervisor: Supervisor, onvif: Watchers, deps: Deps, recordings_dir: PathBuf, live: bool) -> Self {
        let media = deps.hub.clone();
        let files = Arc::new(RecordingFiles::new(recordings_dir));
        let recorder = Arc::new(Recorder::new(recorder::Deps { db: deps.db.clone(), hub: media.clone(), files: files.clone(), bus: deps.bus.clone() }));
        let auto_record = if live {
            AutoRecorders::new(deps.db.clone(), media.clone(), deps.bus.clone(), recorder.clone())
        } else {
            #[cfg(test)]
            {
                AutoRecorders::inert(deps.db.clone(), media.clone(), deps.bus.clone(), recorder.clone())
            }
            #[cfg(not(test))]
            unreachable!("only tests build inert state")
        };
        let retention = Arc::new(Sweeper::new(deps.db.clone(), files.clone(), deps.bus.clone()));
        let exports = Arc::new(Exports::new(deps.db.clone(), deps.credentials.clone(), files.clone()));
        Self {
            db: deps.db,
            credentials: deps.credentials,
            live: deps.live,
            bus: deps.bus,
            supervisor: Arc::new(supervisor),
            onvif: Arc::new(onvif),
            media,
            retention,
            started_at: chrono::Utc::now(),
            metrics: Sampler::default(),
            logs: LogBuffer::default(),
            bind: "127.0.0.1:8090".parse().expect("valid default address"),
            login_limiter: Arc::default(),
            recorder,
            auto_record: Arc::new(auto_record),
            exports,
            recording_files: files,
        }
    }
}
