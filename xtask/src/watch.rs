//! Change detection for `cargo web serve` (mtime polling, std only).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{root, web_dir};

/// Directories and files whose changes trigger a rebuild.
fn watched_paths() -> Vec<PathBuf> {
    let web = web_dir();
    vec![web.join("src"), web.join("style"), web.join("assets"), web.join("index.html"), web.join("Cargo.toml"), root().join("Cargo.toml"), root().join("crates/model/src")]
}

/// Latest modification time under the watched paths.
pub fn latest_mtime() -> SystemTime {
    fn walk(p: &Path, latest: &mut SystemTime) {
        let Ok(meta) = fs::metadata(p) else { return };
        if let Ok(m) = meta.modified() {
            *latest = (*latest).max(m);
        }
        if meta.is_dir()
            && let Ok(rd) = fs::read_dir(p)
        {
            for e in rd.flatten() {
                walk(&e.path(), latest);
            }
        }
    }
    let mut latest = UNIX_EPOCH;
    for p in watched_paths() {
        walk(&p, &mut latest);
    }
    latest
}
