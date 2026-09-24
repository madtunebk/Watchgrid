//! Server configuration from the environment.
//!
//! Sources, first found wins per variable: the process environment (e.g.
//! systemd's `EnvironmentFile=`), `./.env` (development), then
//! `$WATCHGRID_CONFIG` or `/etc/watchgrid/watchgrid.env` (installed
//! system, so `sudo watchgrid …` finds the same settings as the service).

use std::net::SocketAddr;
use std::path::PathBuf;

pub const SYSTEM_CONFIG: &str = "/etc/watchgrid/watchgrid.env";

pub struct Config {
    pub database_url: String,
    /// Where the HTTP API and web UI listen.
    pub bind: SocketAddr,
    /// Credential master key (back it up with the database).
    pub key_file: PathBuf,
    /// Default recordings folder (Settings or `watchgrid storage set-path` can change it).
    pub recordings_dir: PathBuf,
    /// Built web UI (`cargo web build` output).
    pub ui_dir: PathBuf,
}

/// Load `.env` and the system config file into the environment (without
/// overriding what is already set).
pub fn load_env_files() {
    let _ = dotenvy::dotenv();
    let system = std::env::var("WATCHGRID_CONFIG").unwrap_or_else(|_| SYSTEM_CONFIG.into());
    if std::path::Path::new(&system).exists() {
        let _ = dotenvy::from_path(&system);
    }
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let database_url = var("DATABASE_URL").ok_or(format!("DATABASE_URL is not set (see .env or {SYSTEM_CONFIG})"))?;
        let bind = var("WATCHGRID_BIND")
            .unwrap_or_else(|| "127.0.0.1:8090".into())
            .parse()
            .map_err(|e| format!("WATCHGRID_BIND: {e}"))?;
        // Server-owned state (development default: `./data`).
        let data_dir: PathBuf = var("WATCHGRID_DATA_DIR").unwrap_or_else(|| "data".into()).into();
        Ok(Self {
            database_url,
            bind,
            key_file: var("WATCHGRID_KEY_FILE").map_or_else(|| data_dir.join("master.key"), PathBuf::from),
            recordings_dir: var("WATCHGRID_RECORDINGS_DIR").map_or_else(|| data_dir.join("recordings"), PathBuf::from),
            ui_dir: var("WATCHGRID_UI_DIR").unwrap_or_else(|| "dist".into()).into(),
        })
    }
}
