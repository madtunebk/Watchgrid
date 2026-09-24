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
