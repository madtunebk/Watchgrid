//! Backup and restore against a real PostgreSQL (`sqlx::test`).

use sqlx::PgPool;

use super::{create, dump, restore};

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wg-backup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn next_event_id(db: &PgPool) -> String {
    sqlx::query_scalar("INSERT INTO events (camera_id, kind, start_time, end_time, source) VALUES ('cam-a', 'motion', now(), now(), 'test') RETURNING id")
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn a_backup_restores_data_sequences_and_key(db: PgPool) {
    let dir = temp_dir("roundtrip");
    let key = dir.join("master.key");
    std::fs::write(&key, [9u8; 32]).unwrap();
    sqlx::query("INSERT INTO cameras (id, name, host, main_stream_url, recording, motion) VALUES ('cam-a', 'Hall\tway', 'h', 'rtsp://h/', '{}', '{}')")
        .execute(&db)
        .await
        .unwrap();
    for _ in 0..3 {
        next_event_id(&db).await;
    }
    let file = dir.join("b.wgbackup");
    let manifest = create(&db, &key, &file).await.unwrap();
    assert!(manifest.tables.contains(&"cameras".to_string()) && manifest.schema_version > 0);

    let options = db.connect_options().as_ref().clone();
    let new_key = dir.join("restored/master.key");
    let refused = restore(&options, &new_key, &file, false).await.err().unwrap();
    assert!(refused.contains("--replace"), "{refused}");

    // Things happened after the backup; the restore brings back its state.
    sqlx::query("DELETE FROM cameras").execute(&db).await.unwrap();
    restore(&options, &new_key, &file, true).await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM cameras WHERE id = 'cam-a'").fetch_one(&db).await.unwrap();
    assert_eq!(name, "Hall\tway", "COPY escapes survive");
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM events").fetch_one(&db).await.unwrap();
    assert_eq!(events, 3);
    assert_eq!(next_event_id(&db).await, "evt-4", "sequences continue where the backup was");
    assert_eq!(std::fs::read(&new_key).unwrap(), [9u8; 32]);
    let _ = std::fs::remove_dir_all(dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn restore_waits_for_the_server_to_stop(db: PgPool) {
    let dir = temp_dir("locked");
    let key = dir.join("master.key");
    std::fs::write(&key, [1u8; 32]).unwrap();
    let file = dir.join("b.wgbackup");
    create(&db, &key, &file).await.unwrap();
    let options = db.connect_options().as_ref().clone();
    let _server = dump::take_server_lock(&options).await.unwrap().expect("first lock");
    assert!(dump::take_server_lock(&options).await.unwrap().is_none(), "a second server can't start");
    let err = restore(&options, &key, &file, true).await.err().unwrap();
    assert!(err.contains("Stop it first"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_restore_that_fails_changes_nothing(db: PgPool) {
    let dir = temp_dir("atomic");
    let key = dir.join("master.key");
    std::fs::write(&key, [5u8; 32]).unwrap();
    sqlx::query("INSERT INTO cameras (id, name, host, main_stream_url, recording, motion) VALUES ('cam-a', 'Hall', 'h', 'rtsp://h/', '{}', '{}')")
        .execute(&db)
        .await
        .unwrap();
    let good = dir.join("good.wgbackup");
    create(&db, &key, &good).await.unwrap();

    // The same backup with one table broken and another key.
    let mut contents = super::archive::read(&good).unwrap();
    contents.tables.insert("events".into(), b"not\\tCOPY\\tdata at all\n".to_vec());
    contents.key = vec![6u8; 32];
    let bad = dir.join("bad.wgbackup");
    super::archive::write(&bad, &contents).unwrap();

    // Things changed since: they must survive the failed restore.
    sqlx::query("INSERT INTO cameras (id, name, host, main_stream_url, recording, motion) VALUES ('cam-b', 'Garage', 'h', 'rtsp://h/', '{}', '{}')")
        .execute(&db)
        .await
        .unwrap();
    let options = db.connect_options().as_ref().clone();
    let err = restore(&options, &key, &bad, true).await.err().unwrap();
    assert!(err.contains("nothing was changed"), "{err}");

    let cameras: Vec<String> = sqlx::query_scalar("SELECT id FROM cameras ORDER BY id").fetch_all(&db).await.unwrap();
    assert_eq!(cameras, ["cam-a", "cam-b"], "the database is as it was");
    assert_eq!(std::fs::read(&key).unwrap(), [5u8; 32], "the key is as it was");
    let leftovers = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().contains("restoring")).count();
    assert_eq!(leftovers, 0, "the staged key was removed");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_backup_with_a_short_key_is_refused_before_anything() {
    let dir = temp_dir("shortkey");
    let file = dir.join("b.wgbackup");
    let manifest = super::archive::Manifest {
        format: super::archive::FORMAT,
        created_at: chrono::Utc::now(),
        build: "test".into(),
        schema_version: 1,
        tables: vec![],
        sequences: Default::default(),
    };
    super::archive::write(&file, &super::archive::Contents { manifest, key: vec![1, 2, 3], tables: Default::default() }).unwrap();
    let err = super::archive::read(&file).err().unwrap();
    assert!(err.contains("instead of 32"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}
