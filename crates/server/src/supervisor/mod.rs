//! Camera supervisor: one task per enabled camera, started, restarted and
//! stopped as configuration changes — no daemon restart needed.

pub mod backoff;
mod task;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sqlx::PgPool;
use tokio::task::JoinHandle;

use crate::bus::Bus;
use crate::credentials::CredentialStore;
use crate::live::{CameraLive, LiveRegistry};

/// What camera tasks need; cheap to clone.
#[derive(Clone)]
pub struct Deps {
    pub db: PgPool,
    pub credentials: Arc<CredentialStore>,
    pub live: Arc<LiveRegistry>,
    pub bus: Bus,
}

pub struct Supervisor {
    deps: Deps,
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    /// Tests run without connecting to cameras.
    enabled: bool,
}

impl Supervisor {
    pub fn new(deps: Deps) -> Self {
        Self { deps, tasks: Mutex::new(HashMap::new()), enabled: true }
    }

    /// A supervisor that never connects (for tests).
    #[cfg(test)]
    pub fn inert(deps: Deps) -> Self {
        Self { enabled: false, ..Self::new(deps) }
    }

    /// (Re)start supervision of a camera after it was added or changed.
    pub fn apply(&self, id: &str, enabled: bool) {
        self.stop(id);
        if !enabled || !self.enabled {
            return;
        }
        self.deps.live.set(id, CameraLive::connecting());
        let handle = tokio::spawn(task::run(self.deps.clone(), id.to_string()));
        self.tasks.lock().expect("supervisor lock poisoned").insert(id.to_string(), handle);
    }

    /// Stop supervising (camera disabled or deleted).
    pub fn stop(&self, id: &str) {
        if let Some(handle) = self.tasks.lock().expect("supervisor lock poisoned").remove(id) {
            handle.abort();
        }
        self.deps.live.remove(id);
    }
}
