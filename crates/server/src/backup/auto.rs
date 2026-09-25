//! Daily automatic backups next to the server's data, newest few kept.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::PgPool;

use super::EXTENSION;

const EVERY: Duration = Duration::from_secs(24 * 3600);
/// Let the server settle before the first one.
const FIRST_AFTER: Duration = Duration::from_secs(10 * 60);
pub const KEEP: usize = 7;
/// Only these are rotated; backups made by hand are never deleted.
const PREFIX: &str = "watchgrid-auto-";

pub fn start(db: PgPool, key_file: PathBuf, dir: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_AFTER).await;
        loop {
            let out = dir.join(format!("{PREFIX}{}.{EXTENSION}", chrono::Utc::now().format("%Y%m%d-%H%M%S")));
            match super::create(&db, &key_file, &out).await {
                Ok(m) => {
                    tracing::info!(file = %out.display(), tables = m.tables.len(), "daily backup written");
                    prune(&dir, KEEP);
                }
                Err(e) => tracing::warn!("daily backup failed: {e}"),
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Delete all but the newest `keep` automatic backups (names sort by time).
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == EXTENSION) && p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(PREFIX)))
        .collect();
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    for old in &backups[..excess] {
        if let Err(e) = std::fs::remove_file(old) {
            tracing::warn!(file = %old.display(), "cannot remove an old backup: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_and_leaves_other_files() {
        let dir = std::env::temp_dir().join(format!("wg-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for d in 1..=9 {
            std::fs::write(dir.join(format!("watchgrid-auto-2026090{d}-000000.wgbackup")), b"x").unwrap();
        }
        std::fs::write(dir.join("watchgrid-20260901-120000.wgbackup"), b"x").unwrap();
        prune(&dir, 7);
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), 8, "7 automatic + the one made by hand");
        assert!(!left.contains(&"watchgrid-auto-20260901-000000.wgbackup".to_string()));
        assert!(left.contains(&"watchgrid-20260901-120000.wgbackup".to_string()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
