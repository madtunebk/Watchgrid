//! Retention policy persistence (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::RetentionPolicy;

use super::retention;

#[sqlx::test(migrations = "./migrations")]
async fn policy_defaults_to_keeping_everything_then_round_trips(db: PgPool) {
    assert_eq!(retention::load(&db).await.unwrap(), retention::default_policy());
    let p = RetentionPolicy { max_age_days: Some(14), max_usage: Some(500_000_000_000), min_free: Some(50_000_000_000), event_history_days: None };
    retention::save(&db, &p).await.unwrap();
    assert_eq!(retention::load(&db).await.unwrap(), p);
    let p2 = RetentionPolicy { max_age_days: None, ..p };
    retention::save(&db, &p2).await.unwrap();
    assert_eq!(retention::load(&db).await.unwrap(), p2, "updates replace the stored policy");
}

#[sqlx::test(migrations = "./migrations")]
async fn zero_limits_are_rejected(db: PgPool) {
    let zero_days = RetentionPolicy { max_age_days: Some(0), max_usage: None, min_free: None, event_history_days: None };
    assert_eq!(retention::save(&db, &zero_days).await.unwrap_err().status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    let zero_bytes = RetentionPolicy { max_age_days: None, max_usage: Some(0), min_free: None, event_history_days: None };
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

        retention::save(&db, &RetentionPolicy { max_age_days: Some(7), max_usage: None, min_free: None, event_history_days: None }).await.unwrap();
        assert_eq!(sweeper.pass().await.unwrap(), 1);
        assert!(!old.exists() && kept.exists() && fresh.exists());
        let left: Vec<String> = sqlx::query_scalar("SELECT id FROM recordings ORDER BY id").fetch_all(&db).await.unwrap();
        assert_eq!(left, ["fresh", "protected"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_preview_counts_what_the_pass_then_deletes(db: PgPool) {
        let dir = std::env::temp_dir().join(format!("watchgrid-sweep-preview-{}", std::process::id()));
        let files = Arc::new(RecordingFiles::new(dir.clone()));
        add(&db, &files, "old-1", 30).await;
        add(&db, &files, "old-2", 20).await;
        add(&db, &files, "fresh", 1).await;
        let sweeper = Sweeper::new(db.clone(), files, Bus::new());
        let policy = RetentionPolicy { max_age_days: Some(7), max_usage: None, min_free: None, event_history_days: None };

        let preview = sweeper.doomed(&policy).await.unwrap();
        assert_eq!(preview.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["old-1", "old-2"]);
        assert_eq!(sweeper.pass().await.unwrap(), 0, "the preview deleted nothing (no policy saved yet)");
        retention::save(&db, &policy).await.unwrap();
        assert_eq!(sweeper.pass().await.unwrap(), preview.len());
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

mod folder_switch {
    use sqlx::PgPool;

    use crate::recordings::RecordingFiles;
    use crate::storage::location;

    #[sqlx::test(migrations = "./migrations")]
    async fn a_failed_switch_keeps_the_old_folder(db: PgPool) {
        let base = std::env::temp_dir().join(format!("watchgrid-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (old, bad, good) = (base.join("old"), base.join("bad"), base.join("good"));
        for d in [&old, &bad, &good] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(bad.join(".partial"), b"a file, not a folder").unwrap();
        let files = RecordingFiles::new(old.clone());

        assert!(location::apply(&db, &files, bad.to_str().unwrap()).await.is_err());
        assert_eq!(files.root(), old, "still recording to the old folder");
        assert_eq!(location::load(&db).await.unwrap(), None, "nothing saved");

        let now = location::apply(&db, &files, good.to_str().unwrap()).await.unwrap();
        assert_eq!((files.root(), location::load(&db).await.unwrap()), (now.clone(), Some(now.clone())));
        assert!(now.join(".partial").is_dir(), "prepared before use");
        let _ = std::fs::remove_dir_all(base);
    }
}

#[test]
fn event_history_has_its_own_rule_else_follows_recordings() {
    let p = |age: Option<u32>, events: Option<u32>| RetentionPolicy { max_age_days: age, max_usage: None, min_free: None, event_history_days: events };
    assert_eq!(p(None, None).event_days(), 365, "a year without any rule");
    assert_eq!(p(Some(14), None).event_days(), 14, "as long as the recordings");
    assert_eq!(p(Some(14), Some(90)).event_days(), 90, "its own rule wins");
    assert!(retention::validate(&p(None, Some(0))).is_err());
    // Stored before the rule existed: loads as "follow the recordings".
    let old: RetentionPolicy = serde_json::from_str(r#"{"maxAgeDays":30,"maxUsage":null,"minFree":null}"#).unwrap();
    assert_eq!((old.event_history_days, old.event_days()), (None, 30));
}
