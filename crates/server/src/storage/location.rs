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

/// Full check: shape, an existing folder, and that we can really write
/// there. Returns the real path: a symlink is followed, and where it leads
/// must pass the same checks (no link into a system folder).
pub fn check_writable(path: &str) -> Result<PathBuf, String> {
    let asked = check_shape(path)?;
    if !asked.is_dir() {
        return Err(format!("{} does not exist or is not a folder", asked.display()));
    }
    let real = std::fs::canonicalize(&asked).map_err(|e| format!("cannot resolve {}: {e}", asked.display()))?;
    if real != asked {
        check_shape(&real.to_string_lossy()).map_err(|e| format!("{} leads to {}: {e}", asked.display(), real.display()))?;
    }
    probe(&real).map_err(|e| {
        format!("Watchgrid cannot write to {} ({e}). On the server run: sudo watchgrid storage set-path {}", real.display(), real.display())
    })?;
    Ok(real)
}

/// Write and remove a file of our own (a unique name, never an existing file).
pub(crate) fn probe(dir: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let name = format!(".watchgrid-write-test-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let path = dir.join(name);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    let written = f.write_all(b"ok");
    drop(f);
    let _ = std::fs::remove_file(&path);
    written
}

/// One folder switch at a time.
static SWITCHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Switch new recordings to `path`, remembering the choice. Everything is
/// checked and prepared first; on any error the current folder stays in
/// use and nothing is saved. Returns the real path now in use.
pub async fn apply(db: &PgPool, files: &RecordingFiles, path: &str) -> Result<PathBuf, String> {
    let _one = SWITCHING.lock().await;
    let asked = path.to_string();
    let p = tokio::task::spawn_blocking(move || {
        let p = check_writable(&asked)?;
        RecordingFiles::prepare_root(&p).map_err(|e| format!("cannot prepare {}: {e}", p.display()))?;
        Ok::<_, String>(p)
    })
    .await
    .map_err(|e| e.to_string())??;
    save(db, &p).await.map_err(|e| e.to_string())?;
    files.set_root(p.clone());
    tracing::info!("recordings folder is now {}", p.display());
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_relative_system_and_dotted_paths() {
        assert!(check_shape("/volume1/watchgrid").is_ok());
        assert!(check_shape("/home/alice/recordings").is_ok());
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
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "probe removed");
        assert!(check_writable("/definitely/not/here").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_symlink_into_a_system_folder_is_refused() {
        let dir = std::env::temp_dir().join(format!("watchgrid-loc-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join("recordings");
        std::os::unix::fs::symlink("/etc", &link).unwrap();
        let err = check_writable(link.to_str().unwrap()).err().unwrap();
        assert!(err.contains("leads to /etc"), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_whose_partial_is_a_file_is_refused_before_anything_changes() {
        let dir = std::env::temp_dir().join(format!("watchgrid-loc-partial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".partial"), b"not a folder").unwrap();
        assert!(check_writable(dir.to_str().unwrap()).is_ok(), "writable…");
        assert!(RecordingFiles::prepare_root(&dir).is_err(), "…but not usable for recordings");
        let _ = std::fs::remove_dir_all(dir);
    }
}
