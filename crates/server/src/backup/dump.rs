//! Database side of backups: tables through COPY (no pg_dump in the
//! container), sequences, load order and staged migrations.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use futures::TryStreamExt;
use sqlx::postgres::{PgConnectOptions, PgConnection, PgPoolCopyExt};
use sqlx::{Connection, PgPool};

/// Held by a running server; a restore refuses to run while it is taken
/// (and a second server can't start on the same database).
const SERVER_LOCK: i64 = 0x5747_4E56_52; // "WGNVR"

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Watchgrid's tables (everything in `public` but the migration history).
pub async fn tables<'e>(db: impl sqlx::PgExecutor<'e>) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' AND tablename <> '_sqlx_migrations' ORDER BY tablename",
    )
    .fetch_all(db)
    .await
}

/// Highest applied migration, 0 for an empty database.
pub async fn schema_version(db: &mut PgConnection) -> sqlx::Result<i64> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL").fetch_one(&mut *db).await?;
    if !exists {
        return Ok(0);
    }
    sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success").fetch_one(db).await
}

pub async fn sequences<'e>(db: impl sqlx::PgExecutor<'e>) -> sqlx::Result<BTreeMap<String, i64>> {
    let rows: Vec<(String, Option<i64>)> =
        sqlx::query_as("SELECT sequencename::text, last_value FROM pg_sequences WHERE schemaname = 'public'").fetch_all(db).await?;
    Ok(rows.into_iter().filter_map(|(name, v)| v.map(|v| (name, v))).collect())
}

/// A table's rows (on the backup's snapshot connection).
pub async fn copy_out(db: &mut PgConnection, table: &str) -> sqlx::Result<Vec<u8>> {
    let mut stream = db.copy_out_raw(&format!("COPY {} TO STDOUT", quote(table))).await?;
    let mut out = Vec::new();
    while let Some(chunk) = stream.try_next().await? {
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

pub async fn copy_in(db: &PgPool, table: &str, data: &[u8]) -> sqlx::Result<()> {
    let mut copy = db.copy_in_raw(&format!("COPY {} FROM STDIN", quote(table))).await?;
    copy.send(data).await?;
    copy.finish().await.map(|_| ())
}

pub async fn set_sequences(db: &PgPool, values: &BTreeMap<String, i64>) -> sqlx::Result<()> {
    for (name, value) in values {
        sqlx::query("SELECT setval($1::regclass, $2, true)").bind(quote(name)).bind(value).execute(db).await?;
    }
    Ok(())
}

/// `tables` ordered so that every table comes after those it references.
pub async fn load_order(db: &PgPool, tables: &[String]) -> sqlx::Result<Vec<String>> {
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.conrelid::regclass::text, c.confrelid::regclass::text FROM pg_constraint c
         JOIN pg_namespace n ON n.oid = c.connamespace WHERE c.contype = 'f' AND n.nspname = 'public'",
    )
    .fetch_all(db)
    .await?;
    Ok(order(tables, &edges))
}

/// Topological order of `tables` given (table, references) edges.
fn order(tables: &[String], edges: &[(String, String)]) -> Vec<String> {
    let clean = |s: &str| s.trim_matches('"').to_string();
    let mut left: BTreeSet<String> = tables.iter().cloned().collect();
    let mut out = Vec::new();
    while !left.is_empty() {
        let ready: Vec<String> = left
            .iter()
            .filter(|t| edges.iter().all(|(from, to)| clean(from) != **t || clean(to) == **t || !left.contains(&clean(to))))
            .cloned()
            .collect();
        // A cycle can't be ordered: load the rest as they come.
        let batch = if ready.is_empty() { left.iter().cloned().collect() } else { ready };
        for t in batch {
            left.remove(&t);
            out.push(t);
        }
    }
    out
}

/// Migrations up to and including `version` (the backup's schema).
pub async fn migrate_to(db: &PgPool, version: i64) -> Result<(), String> {
    let all = sqlx::migrate!("./migrations");
    if !all.iter().any(|m| m.version == version) {
        return Err(format!("the backup's database schema ({version}) is unknown to this Watchgrid; restore it with the same or a newer version"));
    }
    // Only the fields `migrate!` fills; pinned sqlx version (see Cargo.lock).
    let partial = sqlx::migrate::Migrator {
        migrations: Cow::Owned(all.iter().filter(|m| m.version <= version).cloned().collect()),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    partial.run(db).await.map_err(|e| format!("database migration failed: {e}"))
}

/// Everything after the backup's schema.
pub async fn migrate_all(db: &PgPool) -> Result<(), String> {
    sqlx::migrate!("./migrations").run(db).await.map_err(|e| format!("database migration failed: {e}"))
}

/// Drop every Watchgrid table and sequence (restore --replace).
pub async fn drop_everything(db: &PgPool) -> sqlx::Result<()> {
    let tables: Vec<String> = sqlx::query_scalar("SELECT tablename::text FROM pg_tables WHERE schemaname = 'public'").fetch_all(db).await?;
    for t in tables {
        sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", quote(&t))).execute(db).await?;
    }
    let seqs: Vec<String> = sqlx::query_scalar("SELECT sequencename::text FROM pg_sequences WHERE schemaname = 'public'").fetch_all(db).await?;
    for s in seqs {
        sqlx::query(&format!("DROP SEQUENCE IF EXISTS {} CASCADE", quote(&s))).execute(db).await?;
    }
    Ok(())
}

/// Take the server lock on a connection of its own; `None` if a server
/// (or a restore) already holds it. The lock lasts as long as the connection.
pub async fn take_server_lock(options: &PgConnectOptions) -> Result<Option<PgConnection>, String> {
    let mut conn = PgConnection::connect_with(options).await.map_err(|e| format!("cannot connect to PostgreSQL: {e}"))?;
    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(SERVER_LOCK).fetch_one(&mut conn).await.map_err(|e| e.to_string())?;
    Ok(got.then_some(conn))
}

#[cfg(test)]
mod tests {
    use super::order;

    #[test]
    fn referenced_tables_load_first() {
        let tables: Vec<String> = ["export_jobs", "export_targets", "sessions", "users", "cameras"].map(String::from).to_vec();
        let edges = [("export_jobs".to_string(), "export_targets".to_string()), ("sessions".to_string(), "users".to_string()), ("users".into(), "users".into())];
        let o = order(&tables, &edges);
        let pos = |t: &str| o.iter().position(|x| x == t).unwrap();
        assert!(pos("export_targets") < pos("export_jobs"));
        assert!(pos("users") < pos("sessions"));
        assert_eq!(o.len(), 5);
    }
}
