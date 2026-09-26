//! The `.wgbackup` file: a gzip'd tar with
//!
//! - `manifest.json`: format, schema version, table list, sequence values;
//! - `master.key`: the credential store key (camera and export secrets are
//!   unreadable without it);
//! - `tables/<name>.copy`: each table in PostgreSQL's COPY text format.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};

pub const FORMAT: u32 = 1;
pub const EXTENSION: &str = "wgbackup";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub created_at: DateTime<Utc>,
    /// Watchgrid build that made it.
    pub build: String,
    /// Highest applied migration: the data matches this schema.
    pub schema_version: i64,
    pub tables: Vec<String>,
    /// Sequence name → last value handed out.
    pub sequences: BTreeMap<String, i64>,
}

/// Everything in a backup, in memory (databases here are small: the video
/// itself is not part of a backup).
#[derive(Debug, Clone, PartialEq)]
pub struct Contents {
    pub manifest: Manifest,
    pub key: Vec<u8>,
    /// Table → COPY data.
    pub tables: BTreeMap<String, Vec<u8>>,
}

/// Write the archive next to `path` under a unique temporary name, then
/// rename it into place and make the rename durable. Blocking: call it
/// off the async runtime.
pub fn write(path: &Path, c: &Contents) -> Result<(), String> {
    let unique = format!("partial-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let tmp = path.with_extension(unique);
    let result = write_to(&tmp, c)
        .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| format!("cannot finish {}: {e}", path.display())))
        .and_then(|()| sync_dir(path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Make a rename in `path`'s folder survive a power cut.
fn sync_dir(path: &Path) -> Result<(), String> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::File::open(dir).and_then(|d| d.sync_all()).map_err(|e| format!("cannot sync {}: {e}", dir.display()))
}

fn write_to(path: &Path, c: &Contents) -> Result<(), String> {
    let err = |e: std::io::Error| format!("cannot write {}: {e}", path.display());
    let file = private_file(path).map_err(err)?;
    let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    let manifest = serde_json::to_vec_pretty(&c.manifest).map_err(|e| e.to_string())?;
    append(&mut tar, "manifest.json", &manifest).map_err(err)?;
    append(&mut tar, "master.key", &c.key).map_err(err)?;
    for (name, data) in &c.tables {
        append(&mut tar, &format!("tables/{name}.copy"), data).map_err(err)?;
    }
    let gz = tar.into_inner().map_err(err)?;
    let file = gz.finish().map_err(err)?;
    file.sync_all().map_err(err)
}

/// The backup holds the master key: readable by the owner only. Created
/// fresh (never an existing file, which could have other permissions).
fn private_file(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

fn append<W: Write>(tar: &mut tar::Builder<W>, name: &str, data: &[u8]) -> std::io::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(Utc::now().timestamp().max(0) as u64);
    header.set_cksum();
    tar.append_data(&mut header, name, data)
}

/// Most uncompressed data a backup may hold (a guard against a damaged or
/// hostile file filling memory).
const MAX_TOTAL: u64 = 8 << 30;

pub fn read(path: &Path) -> Result<Contents, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let bad = |what: String| format!("{} is not a Watchgrid backup: {what}", path.display());
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let (mut manifest, mut key, mut tables) = (None, None, BTreeMap::new());
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for entry in archive.entries().map_err(|e| bad(e.to_string()))? {
        let mut entry = entry.map_err(|e| bad(e.to_string()))?;
        let name = entry.path().map_err(|e| bad(e.to_string()))?.to_string_lossy().into_owned();
        if !seen.insert(name.clone()) {
            return Err(bad(format!("`{name}` appears twice")));
        }
        total += entry.size();
        if total > MAX_TOTAL {
            return Err(bad("larger than any Watchgrid backup can be".into()));
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| bad(e.to_string()))?;
        match name.as_str() {
            "manifest.json" => manifest = Some(serde_json::from_slice::<Manifest>(&data).map_err(|e| bad(format!("manifest: {e}")))?),
            "master.key" => key = Some(data),
            _ => {
                if let Some(table) = name.strip_prefix("tables/").and_then(|n| n.strip_suffix(".copy")) {
                    tables.insert(table.to_string(), data);
                }
            }
        }
    }
    let manifest = manifest.ok_or_else(|| bad("no manifest".into()))?;
    if manifest.format != FORMAT {
        return Err(format!("{} uses backup format {}; this Watchgrid reads format {FORMAT}", path.display(), manifest.format));
    }
    let key = key.ok_or_else(|| bad("no master key".into()))?;
    if key.len() != 32 {
        return Err(bad(format!("the master key has {} bytes instead of 32", key.len())));
    }
    if let Some(missing) = manifest.tables.iter().find(|t| !tables.contains_key(*t)) {
        return Err(bad(format!("table `{missing}` is missing")));
    }
    Ok(Contents { manifest, key, tables })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_private() {
        let dir = std::env::temp_dir().join(format!("wg-archive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.wgbackup");
        let c = Contents {
            manifest: Manifest {
                format: FORMAT,
                created_at: Utc::now(),
                build: "test".into(),
                schema_version: 11,
                tables: vec!["cameras".into(), "events".into()],
                sequences: BTreeMap::from([("events_seq".to_string(), 42)]),
            },
            key: vec![7; 32],
            tables: BTreeMap::from([("cameras".to_string(), b"cam-a\tA\n".to_vec()), ("events".to_string(), Vec::new())]),
        };
        write(&path, &c).unwrap();
        assert_eq!(read(&path).unwrap(), c);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::write(&path, b"not a backup").unwrap();
        assert!(read(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
