//! On-disk layout of recordings.
//!
//! ```text
//! <root>/.partial/<id>.mp4.part             being written; never served
//! <root>/<camera>/<YYYY-MM-DD>/<id>.mp4     finalized
//! ```
//!
//! Both live under one root so publishing is an atomic rename.

use std::io;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};

const PARTIAL_DIR: &str = ".partial";

pub struct RecordingFiles {
    root: PathBuf,
}

impl RecordingFiles {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the directories and report files left by an interrupted run.
    pub fn prepare(&self) -> io::Result<()> {
        let partial = self.root.join(PARTIAL_DIR);
        std::fs::create_dir_all(&partial)?;
        let leftovers = std::fs::read_dir(&partial)?.filter_map(Result::ok).count();
        if leftovers > 0 {
            tracing::warn!("{leftovers} incomplete recording file(s) from an interrupted run in {}", partial.display());
        }
        Ok(())
    }

    /// Where a recording is written while in progress.
    pub fn partial_path(&self, id: &str) -> PathBuf {
        self.root.join(PARTIAL_DIR).join(format!("{id}.mp4.part"))
    }

    /// Final location, relative to the root (what the database stores).
    pub fn final_relative(camera_id: &str, id: &str, start: DateTime<Utc>) -> String {
        format!("{camera_id}/{}/{id}.mp4", start.format("%Y-%m-%d"))
    }

    /// Move a completed file into place. The caller has synced its data.
    pub async fn publish(&self, partial: &Path, relative: &str) -> io::Result<PathBuf> {
        let target = self.resolve(relative).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad recording path"))?;
        if let Some(dir) = target.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::fs::rename(partial, &target).await?;
        Ok(target)
    }

    /// Absolute path for a stored relative path; `None` if it would leave the root.
    pub fn resolve(&self, relative: &str) -> Option<PathBuf> {
        let rel = Path::new(relative);
        let safe = rel.components().all(|c| matches!(c, Component::Normal(_)));
        safe.then(|| self.root.join(rel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_paths_cannot_escape_the_root() {
        let files = RecordingFiles::new("/rec".into());
        assert_eq!(files.resolve("cam-a/2026-09-24/x.mp4"), Some(PathBuf::from("/rec/cam-a/2026-09-24/x.mp4")));
        assert_eq!(files.resolve("../etc/passwd"), None);
        assert_eq!(files.resolve("/etc/passwd"), None);
        assert_eq!(files.resolve("cam/../../x"), None);
    }

    #[test]
    fn final_paths_group_by_camera_and_day() {
        let start = "2026-09-24T21:08:16Z".parse().unwrap();
        assert_eq!(RecordingFiles::final_relative("cam-a", "rec-1", start), "cam-a/2026-09-24/rec-1.mp4");
    }
}
