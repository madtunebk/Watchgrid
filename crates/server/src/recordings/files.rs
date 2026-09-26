//! On-disk layout of recordings.
//!
//! ```text
//! <root>/.partial/<id>.mp4.part             being written; never served
//! <root>/<camera>/<YYYY-MM-DD>/<id>.mp4     finalized
//! ```
//!
//! Partial and final files share a root so publishing is an atomic
//! rename. The current root can change at runtime (Settings or CLI); each
//! recording stores the root it was written under, so older clips stay
//! playable where they are.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use chrono::{DateTime, Utc};

const PARTIAL_DIR: &str = ".partial";

pub struct RecordingFiles {
    /// The configured default (recordings stored with no explicit root).
    base: PathBuf,
    current: RwLock<PathBuf>,
}

impl RecordingFiles {
    /// `base` is made absolute: stored roots must be absolute to resolve.
    pub fn new(base: PathBuf) -> Self {
        let base = std::path::absolute(&base).unwrap_or(base);
        Self { current: RwLock::new(base.clone()), base }
    }

    /// Where new recordings go.
    pub fn root(&self) -> PathBuf {
        self.current.read().expect("files lock").clone()
    }

    pub fn set_root(&self, root: PathBuf) {
        *self.current.write().expect("files lock") = root;
    }

    /// Make `root` ready for recordings without using it yet: the
    /// in-progress folder exists and can be written to.
    pub fn prepare_root(root: &Path) -> io::Result<()> {
        let partial = root.join(PARTIAL_DIR);
        std::fs::create_dir_all(&partial)?;
        if !partial.is_dir() {
            return Err(io::Error::other(format!("{} exists but is not a folder", partial.display())));
        }
        crate::storage::location::probe(&partial)
    }

    /// Create the directories and report files left by an interrupted run.
    pub fn prepare(&self) -> io::Result<()> {
        let partial = self.root().join(PARTIAL_DIR);
        Self::prepare_root(&self.root())?;
        let leftovers = std::fs::read_dir(&partial)?.filter_map(Result::ok).count();
        if leftovers > 0 {
            tracing::warn!("{leftovers} incomplete recording file(s) from an interrupted run in {}", partial.display());
        }
        Ok(())
    }

    /// Where a recording is written while in progress, under `root`.
    pub fn partial_path(root: &Path, id: &str) -> PathBuf {
        root.join(PARTIAL_DIR).join(format!("{id}.mp4.part"))
    }

    /// Final location, relative to its root (what the database stores).
    pub fn final_relative(camera_id: &str, id: &str, start: DateTime<Utc>) -> String {
        format!("{camera_id}/{}/{id}.mp4", start.format("%Y-%m-%d"))
    }

    /// Move a completed file into place under `root`. The caller has synced its data.
    pub async fn publish(root: &Path, partial: &Path, relative: &str) -> io::Result<PathBuf> {
        let target = join_safe(root, relative).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad recording path"))?;
        if let Some(dir) = target.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::rename(partial, &target).await?;
        Ok(target)
    }

    /// Absolute path of a stored recording; `None` if the stored values are unsafe.
    pub fn resolve(&self, root: Option<&str>, relative: &str) -> Option<PathBuf> {
        let root = match root {
            None => self.base.clone(),
            Some(r) if Path::new(r).is_absolute() => PathBuf::from(r),
            Some(_) => return None,
        };
        join_safe(&root, relative)
    }
}

/// `root/relative`, refusing anything that could leave `root`.
fn join_safe(root: &Path, relative: &str) -> Option<PathBuf> {
    let rel = Path::new(relative);
    rel.components().all(|c| matches!(c, Component::Normal(_))).then(|| root.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_paths_cannot_escape_their_root() {
        let files = RecordingFiles::new("/rec".into());
        assert_eq!(files.resolve(None, "cam-a/2026-09-24/x.mp4"), Some(PathBuf::from("/rec/cam-a/2026-09-24/x.mp4")));
        assert_eq!(files.resolve(Some("/volume1/wg"), "cam-a/x.mp4"), Some(PathBuf::from("/volume1/wg/cam-a/x.mp4")));
        assert_eq!(files.resolve(None, "../etc/passwd"), None);
        assert_eq!(files.resolve(None, "/etc/passwd"), None);
        assert_eq!(files.resolve(None, "cam/../../x"), None);
        assert_eq!(files.resolve(Some("relative/root"), "x.mp4"), None);
    }

    #[test]
    fn root_changes_apply_to_new_recordings_only() {
        let files = RecordingFiles::new("/rec".into());
        files.set_root("/volume1/wg".into());
        assert_eq!(files.root(), PathBuf::from("/volume1/wg"));
        assert_eq!(files.resolve(None, "a.mp4"), Some(PathBuf::from("/rec/a.mp4")), "old rows keep the default root");
    }

    #[test]
    fn a_relative_default_becomes_absolute() {
        let files = RecordingFiles::new("data/recordings".into());
        assert!(files.root().is_absolute());
        let stored = files.root().to_string_lossy().into_owned();
        assert!(files.resolve(Some(&stored), "a.mp4").is_some(), "what the recorder stores resolves again");
    }

    #[test]
    fn final_paths_group_by_camera_and_day() {
        let start = "2026-09-24T21:08:16Z".parse().unwrap();
        assert_eq!(RecordingFiles::final_relative("cam-a", "rec-1", start), "cam-a/2026-09-24/rec-1.mp4");
    }
}
