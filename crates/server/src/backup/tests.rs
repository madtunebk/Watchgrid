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
