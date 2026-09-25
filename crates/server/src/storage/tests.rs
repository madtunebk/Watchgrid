//! Retention policy persistence (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::RetentionPolicy;

use super::retention;

#[sqlx::test(migrations = "./migrations")]
async fn policy_defaults_to_keeping_everything_then_round_trips(db: PgPool) {
    assert_eq!(retention::load(&db).await.unwrap(), retention::default_policy());
    let p = RetentionPolicy { max_age_days: Some(14), max_usage: Some(500_000_000_000), min_free: Some(50_000_000_000) };
    retention::save(&db, &p).await.unwrap();
    assert_eq!(retention::load(&db).await.unwrap(), p);
    let p2 = RetentionPolicy { max_age_days: None, ..p };
    retention::save(&db, &p2).await.unwrap();
    assert_eq!(retention::load(&db).await.unwrap(), p2, "updates replace the stored policy");
}

#[sqlx::test(migrations = "./migrations")]
async fn zero_limits_are_rejected(db: PgPool) {
    let zero_days = RetentionPolicy { max_age_days: Some(0), max_usage: None, min_free: None };
    assert_eq!(retention::save(&db, &zero_days).await.unwrap_err().status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    let zero_bytes = RetentionPolicy { max_age_days: None, max_usage: Some(0), min_free: None };
    assert!(retention::save(&db, &zero_bytes).await.is_err());
    assert_eq!(retention::load(&db).await.unwrap(), retention::default_policy(), "nothing was stored");
}

mod sweep {
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use sqlx::PgPool;
    use watchgrid_model::{RecordingReason, RetentionPolicy};

    use crate::bus::Bus;
    use crate::recordings::{self, NewRecording, RecordingFiles};
    use crate::storage::{Sweeper, retention};

    async fn add(db: &PgPool, files: &RecordingFiles, id: &str, days_ago: i64) -> std::path::PathBuf {
        let start = Utc::now() - Duration::days(days_ago);
        let path = format!("cam-a/day/{id}.mp4");
        let abs = files.resolve(None, &path).unwrap();
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, b"x").unwrap();
        let row = NewRecording {
            id: id.into(),
            camera_id: "cam-a".into(),
            reason: RecordingReason::Manual,
            start_time: start,
            end_time: start + Duration::seconds(60),
            duration_ms: 60_000,
            file_size: 1,
            path,
            root: None,
            codec: "avc1.640028".into(),
            width: 1280,
            height: 720,
        };
        recordings::insert(db, &row).await.unwrap();
        abs
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn deletes_old_unprotected_recordings_and_their_files(db: PgPool) {
        let dir = std::env::temp_dir().join(format!("watchgrid-sweep-{}", std::process::id()));
        let files = Arc::new(RecordingFiles::new(dir.clone()));
        let old = add(&db, &files, "old", 30).await;
        let kept = add(&db, &files, "protected", 30).await;
        let fresh = add(&db, &files, "fresh", 1).await;
        sqlx::query("UPDATE recordings SET protected = TRUE WHERE id = 'protected'").execute(&db).await.unwrap();
        let sweeper = Sweeper::new(db.clone(), files, Bus::new());

        // No policy: nothing happens.
        assert_eq!(sweeper.pass().await.unwrap(), 0);

        retention::save(&db, &RetentionPolicy { max_age_days: Some(7), max_usage: None, min_free: None }).await.unwrap();
        assert_eq!(sweeper.pass().await.unwrap(), 1);
        assert!(!old.exists() && kept.exists() && fresh.exists());
        let left: Vec<String> = sqlx::query_scalar("SELECT id FROM recordings ORDER BY id").fetch_all(&db).await.unwrap();
        assert_eq!(left, ["fresh", "protected"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_camera_limit_works_without_a_global_policy(db: PgPool) {
        let dir = std::env::temp_dir().join(format!("watchgrid-sweep-cam-{}", std::process::id()));
        let files = Arc::new(RecordingFiles::new(dir.clone()));
        sqlx::query("INSERT INTO cameras (id, name, host, main_stream_url, recording, motion) VALUES ('cam-a', 'A', 'h', 'rtsp://h/', '{\"retentionDays\": 7}', '{}')")
            .execute(&db)
            .await
            .unwrap();
        let old = add(&db, &files, "old", 30).await;
        let fresh = add(&db, &files, "fresh", 1).await;
        let sweeper = Sweeper::new(db.clone(), files, Bus::new());
        assert_eq!(sweeper.pass().await.unwrap(), 1);
        assert!(!old.exists() && fresh.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
