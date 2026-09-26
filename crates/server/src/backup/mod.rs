//! Backups: the database and the master key in one `.wgbackup` file.
//!
//! Recordings (the video files) are not included — they are large, live on
//! their own volume and are best covered by the NAS's own backup. Without
//! the master key a database backup is only half of one: camera passwords
//! and export secrets can't be decrypted.
//!
//! - `watchgrid backup [--out FILE]` at any time (the server may run);
//! - `watchgrid restore FILE [--replace]` with the server stopped;
//! - the server writes one automatically every day and keeps the newest few.

mod archive;
mod auto;
mod dump;

use std::path::{Path, PathBuf};

use chrono::Utc;
use sqlx::PgPool;
use sqlx::postgres::PgConnectOptions;

pub use archive::EXTENSION;
pub use auto::start as start_daily;
pub use dump::take_server_lock;

/// `watchgrid-20260925-181500.wgbackup` in `dir` (made by hand).
pub fn default_path(dir: &Path) -> PathBuf {
    dir.join(format!("watchgrid-{}.{EXTENSION}", Utc::now().format("%Y%m%d-%H%M%S")))
}

/// Write a backup of the database and the master key to `out`.
pub async fn create(db: &PgPool, key_file: &Path, out: &Path) -> Result<archive::Manifest, String> {
    let key = std::fs::read(key_file).map_err(|e| format!("cannot read the master key {}: {e}", key_file.display()))?;
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let db_err = |e: sqlx::Error| format!("cannot read the database: {e}");
    let schema_version = dump::schema_version(db).await.map_err(db_err)?;
    let names = dump::tables(db).await.map_err(db_err)?;
    let mut tables = std::collections::BTreeMap::new();
    for t in &names {
        tables.insert(t.clone(), dump::copy_out(db, t).await.map_err(db_err)?);
    }
    let manifest = archive::Manifest {
        format: archive::FORMAT,
        created_at: Utc::now(),
        build: option_env!("WATCHGRID_BUILD").unwrap_or("dev").into(),
        schema_version,
        tables: names,
        sequences: dump::sequences(db).await.map_err(db_err)?,
    };
    archive::write(out, &archive::Contents { manifest: manifest.clone(), key, tables })?;
    Ok(manifest)
}

/// What a restore did.
pub struct Restored {
    pub manifest: archive::Manifest,
    /// Where the key that was replaced went, if there was a different one.
    pub old_key: Option<PathBuf>,
}

/// Restore into the database at `url` and the key file. The server must be
/// stopped. An existing Watchgrid database is only overwritten with
/// `replace`.
pub async fn restore(options: &PgConnectOptions, key_file: &Path, file: &Path, replace: bool) -> Result<Restored, String> {
    let backup = archive::read(file)?;
    // Held until the restore ends: no server can start meanwhile.
    let _lock = dump::take_server_lock(options).await?.ok_or("Watchgrid is running on this database. Stop it first (Docker: `docker compose stop watchgrid`).")?;
    let db = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect_with(options.clone()).await.map_err(|e| format!("cannot connect to PostgreSQL: {e}"))?;
    let db_err = |e: sqlx::Error| format!("database error: {e}");

    let existing = dump::tables(&db).await.map_err(db_err)?;
    if !existing.is_empty() {
        if !replace {
            return Err(format!(
                "the database already has Watchgrid data ({} tables). Use --replace to delete it and restore the backup instead.",
                existing.len()
            ));
        }
        dump::drop_everything(&db).await.map_err(db_err)?;
    }
    // Rebuild the schema the backup was made with, load, then upgrade.
    dump::migrate_to(&db, backup.manifest.schema_version).await?;
    for t in dump::load_order(&db, &backup.manifest.tables).await.map_err(db_err)? {
        dump::copy_in(&db, &t, &backup.tables[&t]).await.map_err(|e| format!("cannot load table `{t}`: {e}"))?;
    }
    dump::set_sequences(&db, &backup.manifest.sequences).await.map_err(db_err)?;
    dump::migrate_all(&db).await?;
    let old_key = install_key(key_file, &backup.key)?;
    Ok(Restored { manifest: backup.manifest, old_key })
}

/// Put the backup's key in place; a different existing key is kept aside.
fn install_key(key_file: &Path, key: &[u8]) -> Result<Option<PathBuf>, String> {
    let err = |e: std::io::Error| format!("cannot write the master key {}: {e}", key_file.display());
    let mut old = None;
    match std::fs::read(key_file) {
        Ok(current) if current == key => return Ok(None),
        Ok(_) => {
            let aside = key_file.with_extension(format!("key.replaced-{}", Utc::now().format("%Y%m%d-%H%M%S")));
            std::fs::rename(key_file, &aside).map_err(err)?;
            old = Some(aside);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(err(e)),
    }
    if let Some(dir) = key_file.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(key_file).map_err(err)?;
    f.write_all(key).and_then(|()| f.sync_all()).map_err(err)?;
    Ok(old)
}

#[cfg(test)]
mod tests;
