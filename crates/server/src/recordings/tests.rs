//! Recording metadata against a real PostgreSQL (`sqlx::test`).

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use watchgrid_model::RecordingReason;

use super::repo::{self, NewRecording};

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn rec(id: &str, camera: &str, start: &str, secs: i64) -> NewRecording {
    let start_time = at(start);
    NewRecording {
        id: id.into(),
        camera_id: camera.into(),
        reason: RecordingReason::Manual,
        start_time,
        end_time: start_time + Duration::seconds(secs),
        duration_ms: secs * 1000,
        file_size: 1_000_000,
        path: format!("{camera}/2026-09-24/{id}.mp4"),
        root: None,
        codec: "avc1.640028".into(),
        width: 1920,
        height: 1080,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_by_camera_and_overlap(db: PgPool) {
    repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
    repo::insert(&db, &rec("r2", "cam-b", "2026-09-24T11:00:00Z", 60)).await.unwrap();
    repo::insert(&db, &rec("r3", "cam-a", "2026-09-24T23:59:30Z", 60)).await.unwrap();

    let all = repo::list(&db, &[], None, None).await.unwrap();
    assert_eq!(all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r1", "r2", "r3"], "oldest first");

    let cam_a = repo::list(&db, &["cam-a".into()], None, None).await.unwrap();
    assert_eq!(cam_a.len(), 2);

    // A day range catches a recording that runs past midnight.
    let day = repo::list(&db, &[], Some(at("2026-09-25T00:00:00Z")), Some(at("2026-09-26T00:00:00Z"))).await.unwrap();
    assert_eq!(day.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r3"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn read_back_matches_the_model(db: PgPool) {
    repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 90)).await.unwrap();
    let r = repo::get(&db, "r1").await.unwrap().unwrap();
    assert_eq!((r.duration, r.file_size, r.reason, r.protected), (90, 1_000_000, RecordingReason::Manual, false));
    assert_eq!(r.end_time, Some(at("2026-09-24T10:01:30Z")));
    assert_eq!(repo::path(&db, "r1").await.unwrap(), Some((None, "cam-a/2026-09-24/r1.mp4".to_string())));
    assert!(repo::get(&db, "nope").await.unwrap().is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn usage_is_summed_per_camera(db: PgPool) {
    repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
    repo::insert(&db, &rec("r2", "cam-a", "2026-09-23T10:00:00Z", 60)).await.unwrap();
    repo::insert(&db, &rec("r3", "cam-b", "2026-09-24T11:00:00Z", 60)).await.unwrap();
    sqlx::query("UPDATE recordings SET protected = TRUE WHERE id = 'r3'").execute(&db).await.unwrap();

    let usage = repo::usage_by_camera(&db).await.unwrap();
    assert_eq!(usage.len(), 2);
    assert_eq!((usage[0].camera_id.as_str(), usage[0].bytes, usage[0].recordings), ("cam-a", 2_000_000, 2), "largest first");
    assert_eq!(usage[0].oldest, Some(at("2026-09-23T10:00:00Z")));
    assert_eq!(repo::protected_bytes(&db).await.unwrap(), 1_000_000);
}

mod protection_and_delete {
    use sqlx::PgPool;

    use super::rec;
    use crate::recordings::{DeleteError, RecordingFiles, delete_recording, repo};

    /// A recordings folder with the file of `r1` on disk.
    fn folder(name: &str) -> (RecordingFiles, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("watchgrid-{name}-{}", std::process::id()));
        let files = RecordingFiles::new(dir);
        let file = files.resolve(None, "cam-a/2026-09-24/r1.mp4").unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"video").unwrap();
        (files, file)
    }

    async fn event(db: &PgPool, id: &str, recording: &str, protected: bool) {
        sqlx::query("INSERT INTO events (id, camera_id, kind, start_time, end_time, recording_id, source, protected) VALUES ($1, 'cam-a', 'motion', now(), now(), $2, 'test', $3)")
            .bind(id)
            .bind(recording)
            .bind(protected)
            .execute(db)
            .await
            .unwrap();
    }

    async fn set_event(db: &PgPool, id: &str, protected: bool) {
        sqlx::query("UPDATE events SET protected = $2 WHERE id = $1").bind(id).bind(protected).execute(db).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unprotecting_one_event_keeps_a_clip_another_protects(db: PgPool) {
        let (files, file) = folder("shared-protect");
        repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
        event(&db, "a", "r1", true).await;
        event(&db, "b", "r1", true).await;

        set_event(&db, "a", false).await;
        let r = repo::get(&db, "r1").await.unwrap().unwrap();
        assert_eq!((r.protected, r.protected_by_events, r.is_protected()), (false, 1, true));
        assert!(repo::retention_candidates(&db).await.unwrap().is_empty(), "retention skips it");
        assert_eq!(delete_recording(&db, &files, "r1").await, Err(DeleteError::Protected));
        assert!(file.exists());

        set_event(&db, "b", false).await;
        assert_eq!(repo::retention_candidates(&db).await.unwrap().len(), 1, "no protection left");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn protecting_an_event_while_recording_holds_after_the_clip_is_saved(db: PgPool) {
        event(&db, "a", "r1", true).await; // the clip is still being written
        repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
        assert!(repo::get(&db, "r1").await.unwrap().unwrap().is_protected());
        assert_eq!(repo::protected_bytes(&db).await.unwrap(), 1_000_000);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manual_and_event_protection_are_independent(db: PgPool) {
        repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
        event(&db, "a", "r1", true).await;
        repo::set_protected(&db, "r1", true).await.unwrap();
        repo::set_protected(&db, "r1", false).await.unwrap();
        assert!(repo::get(&db, "r1").await.unwrap().unwrap().is_protected(), "still protected by the event");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn deleting_a_recording_keeps_its_events_without_video(db: PgPool) {
        let (files, file) = folder("delete-links");
        repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
        event(&db, "a", "r1", false).await;
        event(&db, "b", "r1", false).await;

        assert_eq!(delete_recording(&db, &files, "r1").await, Ok(true));
        assert!(!file.exists());
        assert!(repo::get(&db, "r1").await.unwrap().is_none());
        let links: Vec<Option<String>> = sqlx::query_scalar("SELECT recording_id FROM events ORDER BY id").fetch_all(&db).await.unwrap();
        assert_eq!(links, [None, None], "events stay, unlinked");
        assert_eq!(delete_recording(&db, &files, "r1").await, Ok(false), "already gone");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_failed_file_removal_changes_nothing(db: PgPool) {
        let (files, file) = folder("delete-fails");
        repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
        event(&db, "a", "r1", false).await;
        // A directory where the file should be: remove_file fails.
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir_all(file.join("x")).unwrap();

        assert!(matches!(delete_recording(&db, &files, "r1").await, Err(DeleteError::Failed(_))));
        assert!(repo::get(&db, "r1").await.unwrap().is_some(), "row kept");
        let link: Option<String> = sqlx::query_scalar("SELECT recording_id FROM events").fetch_one(&db).await.unwrap();
        assert_eq!(link.as_deref(), Some("r1"), "link kept");
        std::fs::remove_dir_all(&file).unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn saved_clips_list_their_events(db: PgPool) {
    repo::insert(&db, &rec("r1", "cam-a", "2026-09-24T10:00:00Z", 60)).await.unwrap();
    repo::insert(&db, &rec("r2", "cam-a", "2026-09-24T11:00:00Z", 60)).await.unwrap();
    for (id, secs) in [("b", 30), ("a", 10)] {
        sqlx::query("INSERT INTO events (id, camera_id, kind, start_time, end_time, recording_id, source) VALUES ($1, 'cam-a', 'motion', $2, $2, 'r1', 'test')")
            .bind(id)
            .bind(at("2026-09-24T10:00:00Z") + Duration::seconds(secs))
            .execute(&db)
            .await
            .unwrap();
    }
    let list = repo::list(&db, &[], None, None).await.unwrap();
    assert_eq!(list[0].event_ids, ["a", "b"], "oldest first");
    assert!(list[1].event_ids.is_empty());
    assert_eq!(repo::get(&db, "r1").await.unwrap().unwrap().event_ids, ["a", "b"]);
}
