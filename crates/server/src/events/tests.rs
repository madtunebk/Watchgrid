//! Event journal and queries against a real PostgreSQL (`sqlx::test`).

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use watchgrid_model::{EventQuery, EventType, RecordingReason};

use super::{journal, repo};
use crate::bus::BusEvent;
use crate::recordings::{self, NewRecording};

const TZ: &str = "Europe/Bucharest";

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

async fn all(db: &PgPool) -> Vec<watchgrid_model::Event> {
    repo::list(db, &EventQuery::default(), TZ).await.unwrap().0
}

fn offline(cam: &str, at: &str) -> BusEvent {
    BusEvent::CameraOffline { camera_id: cam.into(), reason: "connection refused".into(), at: t(at) }
}

fn online(cam: &str, at: &str) -> BusEvent {
    BusEvent::CameraOnline { camera_id: cam.into(), at: t(at) }
}

#[sqlx::test(migrations = "./migrations")]
async fn an_outage_is_one_event_closed_by_reconnecting(db: PgPool) {
    assert!(!journal::handle(&db, &mut journal::Links::default(), &online("cam-a", "2026-09-24T09:00:00Z")).await.unwrap(), "first connect is not an event");
    assert!(journal::handle(&db, &mut journal::Links::default(), &offline("cam-a", "2026-09-24T10:00:00Z")).await.unwrap());
    assert!(!journal::handle(&db, &mut journal::Links::default(), &offline("cam-a", "2026-09-24T10:00:30Z")).await.unwrap(), "retries don't duplicate the outage");

    let open = all(&db).await;
    assert_eq!(open.len(), 1);
    assert_eq!((open[0].kind, open[0].end_time), (EventType::CameraOffline, None));
    assert_eq!(open[0].source, "Supervisor: connection refused");

    assert!(journal::handle(&db, &mut journal::Links::default(), &online("cam-a", "2026-09-24T10:05:00Z")).await.unwrap());
    let events = all(&db).await;
    assert_eq!(events.iter().map(|e| e.kind).collect::<Vec<_>>(), [EventType::CameraOnline, EventType::CameraOffline], "newest first");
    assert_eq!(events[1].end_time, Some(t("2026-09-24T10:05:00Z")));
    assert_eq!(events[1].duration, 300);
}

#[sqlx::test(migrations = "./migrations")]
async fn disabling_a_camera_closes_its_outage(db: PgPool) {
    journal::handle(&db, &mut journal::Links::default(), &offline("cam-a", "2026-09-24T10:00:00Z")).await.unwrap();
    journal::handle(&db, &mut journal::Links::default(), &BusEvent::CameraStopped { camera_id: "cam-a".into(), at: t("2026-09-24T10:01:00Z") }).await.unwrap();
    assert_eq!(all(&db).await[0].end_time, Some(t("2026-09-24T10:01:00Z")));
}

fn rec(id: &str, start: DateTime<Utc>, secs: i64) -> NewRecording {
    NewRecording {
        id: id.into(),
        camera_id: "cam-a".into(),
        reason: RecordingReason::Manual,
        start_time: start,
        end_time: start + Duration::seconds(secs),
        duration_ms: secs * 1000,
        file_size: 1,
        path: format!("cam-a/d/{id}.mp4"),
        root: None,
        codec: "avc1.640028".into(),
        width: 1,
        height: 1,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn a_recording_event_spans_its_recording(db: PgPool) {
    let start = t("2026-09-24T12:00:00Z");
    let started = BusEvent::RecordingStarted { camera_id: "cam-a".into(), recording_id: "rec-1".into(), reason: RecordingReason::Manual, at: start };
    journal::handle(&db, &mut journal::Links::default(), &started).await.unwrap();
    let e = &all(&db).await[0];
    assert_eq!((e.kind, e.recording_id.as_deref(), e.end_time), (EventType::Manual, Some("rec-1"), None));

    recordings::insert(&db, &rec("rec-1", start, 42)).await.unwrap();
    let stopped = BusEvent::RecordingStopped { camera_id: "cam-a".into(), recording_id: Some("rec-1".into()), error: None, at: start + Duration::seconds(50) };
    journal::handle(&db, &mut journal::Links::default(), &stopped).await.unwrap();
    let e = &all(&db).await[0];
    assert_eq!(e.end_time, Some(start + Duration::seconds(42)), "the recording's own end, not the stop time");
    assert_eq!(e.duration, 42);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_failed_recording_keeps_the_event_without_a_clip(db: PgPool) {
    let at = t("2026-09-24T12:00:00Z");
    journal::handle(&db, &mut journal::Links::default(), &BusEvent::RecordingStarted { camera_id: "cam-a".into(), recording_id: "rec-x".into(), reason: RecordingReason::Manual, at }).await.unwrap();
    let stopped = BusEvent::RecordingStopped { camera_id: "cam-a".into(), recording_id: None, error: Some("the disk is full".into()), at: at + Duration::seconds(5) };
    journal::handle(&db, &mut journal::Links::default(), &stopped).await.unwrap();
    let e = &all(&db).await[0];
    assert_eq!((e.kind, e.recording_id.as_deref()), (EventType::Manual, None));
    assert_eq!(e.source, "Manual recording — ended early: the disk is full");
}

#[sqlx::test(migrations = "./migrations")]
async fn restart_closes_recording_events_left_open(db: PgPool) {
    let at = t("2026-09-24T12:00:00Z");
    for (cam, rec_id) in [("cam-a", "rec-1"), ("cam-b", "rec-lost")] {
        let e = BusEvent::RecordingStarted { camera_id: cam.into(), recording_id: rec_id.into(), reason: RecordingReason::Manual, at };
        journal::handle(&db, &mut journal::Links::default(), &e).await.unwrap();
    }
    journal::handle(&db, &mut journal::Links::default(), &offline("cam-c", "2026-09-24T12:00:00Z")).await.unwrap();
    recordings::insert(&db, &rec("rec-1", at, 30)).await.unwrap();

    assert_eq!(repo::close_stale(&db).await.unwrap(), 2);
    let events = all(&db).await;
    let by_cam = |c: &str| events.iter().find(|e| e.camera_id == c).unwrap().clone();
    assert_eq!((by_cam("cam-a").end_time, by_cam("cam-a").recording_id.as_deref()), (Some(at + Duration::seconds(30)), Some("rec-1")));
    assert_eq!((by_cam("cam-b").end_time, by_cam("cam-b").recording_id), (Some(at), None), "finalized file missing: no clip");
    assert_eq!(by_cam("cam-c").end_time, None, "outages stay open until the camera reconnects");
}

#[sqlx::test(migrations = "./migrations")]
async fn queries_filter_page_and_link_neighbours(db: PgPool) {
    for (i, cam) in ["cam-a", "cam-b", "cam-a", "cam-b"].iter().enumerate() {
        let at = t("2026-09-24T10:00:00Z") + Duration::hours(i as i64);
        repo::instant(&db, cam, if i % 2 == 0 { EventType::CameraOnline } else { EventType::Manual }, at, "x").await.unwrap();
    }
    let (page, total) = repo::list(&db, &EventQuery { limit: Some(2), offset: Some(1), ..Default::default() }, TZ).await.unwrap();
    assert_eq!(total, 4);
    assert_eq!(page.iter().map(|e| e.start_time).collect::<Vec<_>>(), [t("2026-09-24T12:00:00Z"), t("2026-09-24T11:00:00Z")]);

    let q = EventQuery { camera_id: Some("cam-a".into()), kinds: vec![EventType::CameraOnline], ..Default::default() };
    assert_eq!(repo::list(&db, &q, TZ).await.unwrap().1, 2);
    let q = EventQuery { from: Some(t("2026-09-24T11:00:00Z")), to: Some(t("2026-09-24T13:00:00Z")), ..Default::default() };
    assert_eq!(repo::list(&db, &q, TZ).await.unwrap().1, 2, "from inclusive, to exclusive");

    // Hours are local to the server: ask PostgreSQL which local hour 10:00Z is.
    let local: f64 = sqlx::query_scalar("SELECT EXTRACT(HOUR FROM $1::timestamptz AT TIME ZONE $2)::float8")
        .bind(t("2026-09-24T10:00:00Z"))
        .bind(TZ)
        .fetch_one(&db)
        .await
        .unwrap();
    let h = local as u8;
    let q = EventQuery { hours: Some((h, (h + 1) % 24)), ..Default::default() };
    let (only, _) = repo::list(&db, &q, TZ).await.unwrap();
    assert_eq!(only.iter().map(|e| e.start_time).collect::<Vec<_>>(), [t("2026-09-24T10:00:00Z")]);

    let middle = repo::list(&db, &EventQuery::default(), TZ).await.unwrap().0[1].clone();
    let (prev, next) = repo::neighbours(&db, &middle).await.unwrap();
    let all = repo::list(&db, &EventQuery::default(), TZ).await.unwrap().0;
    assert_eq!((prev, next), (Some(all[2].id.clone()), Some(all[0].id.clone())));
}

#[sqlx::test(migrations = "./migrations")]
async fn event_recordings_link_to_their_detections(db: PgPool) {
    let mut links = journal::Links::default();
    let at = t("2026-09-24T22:00:00Z");
    let motion = BusEvent::DetectionStarted { camera_id: "cam-a".into(), kind: EventType::Motion, topic: "RuleEngine/CellMotionDetector/Motion".into(), at };
    journal::handle(&db, &mut links, &motion).await.unwrap();
    let started = BusEvent::RecordingStarted { camera_id: "cam-a".into(), recording_id: "rec-m".into(), reason: RecordingReason::Motion, at };
    journal::handle(&db, &mut links, &started).await.unwrap();
    let person = BusEvent::DetectionStarted { camera_id: "cam-a".into(), kind: EventType::Person, topic: "RuleEngine/PeopleDetector/People".into(), at: at + Duration::seconds(3) };
    journal::handle(&db, &mut links, &person).await.unwrap();

    let events = all(&db).await;
    assert_eq!(events.len(), 2, "no separate 'manual' event for an event recording");
    assert!(events.iter().all(|e| e.recording_id.as_deref() == Some("rec-m")), "both detections point at the clip");

    let stopped = BusEvent::RecordingStopped { camera_id: "cam-a".into(), recording_id: None, error: Some("x".into()), at };
    journal::handle(&db, &mut links, &stopped).await.unwrap();
    assert!(all(&db).await.iter().all(|e| e.recording_id.is_none()), "unsaved clip: links removed");
}

#[sqlx::test(migrations = "./migrations")]
async fn events_with_a_clip_point_at_their_moment_in_it(db: PgPool) {
    let start = t("2026-09-24T12:00:00Z");
    recordings::insert(&db, &rec("rec-1", start, 30)).await.unwrap();
    let mut links = journal::Links::default();
    let motion = BusEvent::DetectionStarted { camera_id: "cam-a".into(), kind: EventType::Motion, topic: "m".into(), at: start + Duration::seconds(6) };
    let started = BusEvent::RecordingStarted { camera_id: "cam-a".into(), recording_id: "rec-1".into(), reason: RecordingReason::Motion, at: start };
    journal::handle(&db, &mut links, &motion).await.unwrap();
    journal::handle(&db, &mut links, &started).await.unwrap();
    repo::instant(&db, "cam-a", EventType::CameraOnline, start, "x").await.unwrap();

    let events = all(&db).await;
    let motion = events.iter().find(|e| e.kind == EventType::Motion).unwrap();
    assert_eq!(motion.thumbnail.as_deref(), Some("/api/v1/recordings/rec-1/media#t=6.5"));
    assert!(events.iter().find(|e| e.kind == EventType::CameraOnline).unwrap().thumbnail.is_none(), "no clip, no thumbnail");
}

#[sqlx::test(migrations = "./migrations")]
async fn detections_shorter_than_the_minimum_are_dropped(db: PgPool) {
    let at = t("2026-09-24T22:00:00Z");
    for (kind, secs) in [(EventType::Motion, 1), (EventType::Person, 5)] {
        repo::open_from(&db, "cam-a", kind, at, "ONVIF: x", None, "onvif").await.unwrap();
        repo::close_detection(&db, "cam-a", kind, at + Duration::seconds(secs), 2).await.unwrap();
    }
    let kinds: Vec<EventType> = all(&db).await.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [EventType::Person], "the 1 s motion blip is gone, the 5 s person stays");
}

#[sqlx::test(migrations = "./migrations")]
async fn old_finished_unprotected_events_are_purged(db: PgPool) {
    let old = chrono::Utc::now() - Duration::days(40);
    repo::instant(&db, "cam-a", EventType::CameraOnline, old, "old").await.unwrap();
    repo::instant(&db, "cam-a", EventType::CameraOnline, old, "old but protected").await.unwrap();
    sqlx::query("UPDATE events SET protected = TRUE WHERE source = 'old but protected'").execute(&db).await.unwrap();
    repo::open(&db, "cam-b", EventType::CameraOffline, old, "still offline", None).await.unwrap();
    repo::instant(&db, "cam-a", EventType::CameraOnline, chrono::Utc::now(), "new").await.unwrap();

    assert_eq!(repo::purge_older_than(&db, 30).await.unwrap(), 1);
    let mut left: Vec<String> = all(&db).await.into_iter().map(|e| e.source).collect();
    left.sort();
    assert_eq!(left, ["new", "old but protected", "still offline"]);
}
