//! Server configuration from the environment (and an optional `.env`).

use std::net::SocketAddr;
use std::path::PathBuf;

pub struct Config {
    pub database_url: String,
    /// Where the HTTP API and web UI listen.
    pub bind: SocketAddr,
    /// Server-owned files: the credential master key, later recordings metadata caches.
    pub data_dir: PathBuf,
    /// Built web UI (`cargo web build` output).
    pub ui_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let database_url = var("DATABASE_URL").ok_or("DATABASE_URL is not set (see .env)")?;
        let bind = var("WATCHGRID_BIND")
            .unwrap_or_else(|| "127.0.0.1:8090".into())
            .parse()
            .map_err(|e| format!("WATCHGRID_BIND: {e}"))?;
        Ok(Self {
            database_url,
            bind,
            data_dir: var("WATCHGRID_DATA_DIR").unwrap_or_else(|| "data".into()).into(),
            ui_dir: var("WATCHGRID_UI_DIR").unwrap_or_else(|| "dist".into()).into(),
        })
    }
}
