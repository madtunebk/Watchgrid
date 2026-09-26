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
    // One read-only snapshot for everything: the server may keep writing,
    // but every table (and the schema version) is from the same moment.
    let mut tx = db.begin().await.map_err(db_err)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY").execute(&mut *tx).await.map_err(db_err)?;
    let schema_version = dump::schema_version(&mut tx).await.map_err(db_err)?;
    let names = dump::tables(&mut *tx).await.map_err(db_err)?;
    let mut tables = std::collections::BTreeMap::new();
    for t in &names {
        tables.insert(t.clone(), dump::copy_out(&mut tx, t).await.map_err(db_err)?);
    }
    let sequences = dump::sequences(&mut *tx).await.map_err(db_err)?;
    tx.rollback().await.map_err(db_err)?;
    let manifest = archive::Manifest {
        format: archive::FORMAT,
        created_at: Utc::now(),
        build: option_env!("WATCHGRID_BUILD").unwrap_or("dev").into(),
        schema_version,
        tables: names,
        sequences,
    };
    // Compressing and writing block: off the async workers.
    let (out, contents) = (out.to_path_buf(), archive::Contents { manifest: manifest.clone(), key, tables });
    tokio::task::spawn_blocking(move || archive::write(&out, &contents)).await.map_err(|e| e.to_string())??;
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
///
/// Safe to fail at any point: everything is checked before anything
/// changes, the database is replaced in one transaction (DROP and CREATE
/// are transactional in PostgreSQL), and the new key is written and synced
/// before that transaction commits.
pub async fn restore(options: &PgConnectOptions, key_file: &Path, file: &Path, replace: bool) -> Result<Restored, String> {
    use sqlx::Connection;

    // Preflight: the file (format, entries, key) and the schema.
    let backup = archive::read(file)?;
    let version = backup.manifest.schema_version;
    if !dump::known_schema(version) {
        return Err(format!("the backup's database schema ({version}) is unknown to this Watchgrid; restore it with the same or a newer version"));
    }
    // Held until the restore ends: no server can start meanwhile.
    let _lock = dump::take_server_lock(options).await?.ok_or("Watchgrid is running on this database. Stop it first (Docker: `docker compose stop watchgrid`).")?;
    let mut conn = sqlx::PgConnection::connect_with(options).await.map_err(|e| format!("cannot connect to PostgreSQL: {e}"))?;
    let db_err = |e: sqlx::Error| format!("database error: {e}");

    let existing = dump::tables(&mut conn).await.map_err(db_err)?;
    if !existing.is_empty() {
        if !replace {
            return Err(format!(
                "the database already has Watchgrid data ({} tables). Use --replace to delete it and restore the backup instead.",
                existing.len()
            ));
        }
        if !dump::is_watchgrid(&mut conn).await.map_err(db_err)? {
            return Err("this database has tables that Watchgrid didn't create; restore into Watchgrid's own (or an empty) database".into());
        }
    }

    // The key is ready on disk before the database changes.
    let staged = key::stage(key_file, &backup.key)?;
    let result: Result<(), String> = async {
        let mut tx = Connection::begin(&mut conn).await.map_err(db_err)?;
        let db: &mut sqlx::PgConnection = &mut tx;
        if !existing.is_empty() {
            dump::drop_everything(db).await.map_err(db_err)?;
        }
        // Rebuild the schema the backup was made with, load, then upgrade.
        dump::migrate_to(db, version).await?;
        let order: Vec<String> = dump::load_order(&mut *db, &backup.manifest.tables).await.map_err(db_err)?;
        for t in order {
            let data: &[u8] = &backup.tables[&t];
            dump::copy_in(db, &t, data).await.map_err(|e| format!("cannot load table `{t}`: {e}"))?;
        }
        dump::set_sequences(db, &backup.manifest.sequences).await.map_err(db_err)?;
        dump::migrate_all(db).await?;
        tx.commit().await.map_err(db_err)
    }
    .await;
    if let Err(e) = result {
        key::discard(staged);
        return Err(format!("{e} — nothing was changed"));
    }
    let old_key = key::install(key_file, staged)?;
    Ok(Restored { manifest: backup.manifest, old_key })
}

/// The master key in two steps: `stage` writes and syncs the new key next
/// to the old one; `install` swaps it in with a rename (atomic).
mod key {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};

    use chrono::Utc;

    /// `None`: the key in place is already this one.
    pub struct Staged(Option<PathBuf>);

    fn err(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
        move |e| format!("cannot write the master key {}: {e}", path.display())
    }

    pub fn stage(key_file: &Path, key: &[u8]) -> Result<Staged, String> {
        if std::fs::read(key_file).is_ok_and(|current| current == key) {
            return Ok(Staged(None));
        }
        if let Some(dir) = key_file.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(err(key_file))?;
        }
        let tmp = key_file.with_extension(format!("key.restoring-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).map_err(err(&tmp))?;
        f.write_all(key).and_then(|()| f.sync_all()).map_err(err(&tmp))?;
        Ok(Staged(Some(tmp)))
    }

    pub fn discard(staged: Staged) {
        if let Some(tmp) = staged.0 {
            let _ = std::fs::remove_file(tmp);
        }
    }

    /// Returns where a different old key was kept.
    pub fn install(key_file: &Path, staged: Staged) -> Result<Option<PathBuf>, String> {
        let Some(tmp) = staged.0 else { return Ok(None) };
        let mut old = None;
        if key_file.exists() {
            // A copy of the old key, kept aside (the rename below replaces it).
            let aside = key_file.with_extension(format!("key.replaced-{}", Utc::now().format("%Y%m%d-%H%M%S")));
            std::fs::copy(key_file, &aside).map_err(err(key_file))?;
            old = Some(aside);
        }
        std::fs::rename(&tmp, key_file).map_err(err(key_file))?;
        let dir = key_file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        std::fs::File::open(dir).and_then(|d| d.sync_all()).map_err(err(key_file))?;
        Ok(old)
    }
}

#[cfg(test)]
mod tests;
