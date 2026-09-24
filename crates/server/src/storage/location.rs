//! Choosing the recordings folder at runtime (Settings or CLI).
//!
//! The web UI may only pick a folder the service can already write to;
//! granting write access to a new place is root's job (`watchgrid storage
//! set-path` as root, which also opens the systemd sandbox).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::recordings::RecordingFiles;
use crate::settings;

const KEY: &str = "storage_path";

/// System locations that must never hold recordings.
const FORBIDDEN: [&str; 12] = ["/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/root", "/run", "/sbin", "/sys", "/usr"];

#[derive(Serialize, Deserialize)]
struct Stored {
    path: String,
}

/// The folder chosen in Settings/CLI, if any.
pub async fn load(db: &PgPool) -> sqlx::Result<Option<PathBuf>> {
    Ok(settings::load::<Stored>(db, KEY).await?.map(|s| PathBuf::from(s.path)))
}

pub async fn save(db: &PgPool, path: &Path) -> sqlx::Result<()> {
    settings::save(db, KEY, &Stored { path: path.to_string_lossy().into_owned() }).await
}

/// Shape checks only (no filesystem access).
pub fn check_shape(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path.trim());
    if !p.is_absolute() {
        return Err("Use an absolute path, e.g. /volume1/watchgrid".into());
    }
    if p.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err("The path may not contain . or ..".into());
    }
    if p == Path::new("/") || FORBIDDEN.iter().any(|f| p.starts_with(f)) {
        return Err(format!("{} is a system folder", p.display()));
    }
    Ok(p.to_path_buf())
}

/// Full check: shape, an existing folder, and that we can really write there.
pub fn check_writable(path: &str) -> Result<PathBuf, String> {
    let p = check_shape(path)?;
    if !p.is_dir() {
        return Err(format!("{} does not exist or is not a folder", p.display()));
    }
    let probe = p.join(".watchgrid-write-test");
    std::fs::write(&probe, b"ok").map_err(|e| {
        format!("Watchgrid cannot write to {} ({e}). On the server run: sudo watchgrid storage set-path {}", p.display(), p.display())
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(p)
}

/// Switch new recordings to `path` (checked), remembering the choice.
pub async fn apply(db: &PgPool, files: &RecordingFiles, path: &str) -> Result<PathBuf, String> {
    let p = check_writable(path)?;
    save(db, &p).await.map_err(|e| e.to_string())?;
    files.set_root(p.clone());
    files.prepare().map_err(|e| e.to_string())?;
    tracing::info!("recordings folder is now {}", p.display());
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_relative_system_and_dotted_paths() {
        assert!(check_shape("/volume1/watchgrid").is_ok());
        assert!(check_shape("/home/nobus/recordings").is_ok());
        assert!(check_shape("recordings").is_err());
        assert!(check_shape("/").is_err());
        assert!(check_shape("/etc/watchgrid").is_err());
        assert!(check_shape("/usr/share/x").is_err());
        assert!(check_shape("/volume1/../etc").is_err());
    }

    #[test]
    fn writable_check_uses_the_filesystem() {
        let dir = std::env::temp_dir().join(format!("watchgrid-loc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_writable(dir.to_str().unwrap()).is_ok());
        assert!(!dir.join(".watchgrid-write-test").exists(), "probe removed");
        assert!(check_writable("/definitely/not/here").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
