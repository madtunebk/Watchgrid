//! Camera service tests against a real PostgreSQL. `sqlx::test` creates a
//! fresh database per test (from DATABASE_URL) and applies the migrations.

use sqlx::PgPool;
use watchgrid_model::{CameraInput, CameraStatus, MotionSettings, OnvifConfig, RecordingSettings};

use super::service;
use crate::credentials::CredentialStore;
use crate::state::AppState;

fn state(db: PgPool) -> AppState {
    AppState::inert(db, CredentialStore::from_key(&[3u8; 32]))
}

fn input(name: &str) -> CameraInput {
    CameraInput {
        name: name.into(),
        description: String::new(),
        location: "Entrance".into(),
        enabled: true,
        host: "192.168.1.26".into(),
        username: "admin".into(),
        password: Some("hunter2".into()),
        main_stream_url: "rtsp://192.168.1.26:554/stream1".into(),
        sub_stream_url: None,
        onvif: Some(OnvifConfig { url: "http://192.168.1.26/onvif/device_service".into(), username: "admin".into(), password: Some("onvifpw".into()) }),
        recording: RecordingSettings::default(),
        motion: MotionSettings::default(),
    }
}

async fn stored_secrets(db: &PgPool, id: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    sqlx::query_as("SELECT password_enc, onvif_password_enc FROM cameras WHERE id = $1").bind(id).fetch_one(db).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn create_then_read_back(db: PgPool) {
    let s = state(db);
    let cam = service::create(&s, input("Front Door")).await.unwrap();
    assert_eq!(cam.id, "cam-front-door");
    assert_eq!(cam.status, CameraStatus::Offline, "no supervisor yet: not connected");
    assert!(cam.sub_stream.is_none(), "single-stream camera is valid");
    assert_eq!(service::list(&s).await.unwrap().len(), 1);
    assert_eq!(service::get(&s, "cam-front-door").await.unwrap().name, "Front Door");
}

#[sqlx::test(migrations = "./migrations")]
async fn secrets_are_encrypted_and_never_returned(db: PgPool) {
    let s = state(db.clone());
    let cam = service::create(&s, input("Front Door")).await.unwrap();
    assert!(cam.onvif.as_ref().unwrap().password.is_none());

    let (pw, onvif_pw) = stored_secrets(&db, &cam.id).await;
    let (pw, onvif_pw) = (pw.unwrap(), onvif_pw.unwrap());
    assert!(!pw.windows(7).any(|w| w == b"hunter2"), "password must not be stored in plaintext");
    assert_eq!(s.credentials.open("camera:cam-front-door:password", &pw).unwrap(), b"hunter2");
    assert_eq!(s.credentials.open("camera:cam-front-door:onvif-password", &onvif_pw).unwrap(), b"onvifpw");
}

#[sqlx::test(migrations = "./migrations")]
async fn update_without_password_keeps_the_stored_one(db: PgPool) {
    let s = state(db.clone());
    let cam = service::create(&s, input("Front Door")).await.unwrap();
    let before = stored_secrets(&db, &cam.id).await;

    let mut edit = input("Front Door");
    edit.password = None;
    edit.onvif.as_mut().unwrap().password = None;
    edit.location = "Porch".into();
    let updated = service::update(&s, &cam.id, edit).await.unwrap();
    assert_eq!(updated.location, "Porch");
    assert_eq!(stored_secrets(&db, &cam.id).await, before);
}

#[sqlx::test(migrations = "./migrations")]
async fn removing_onvif_clears_its_password(db: PgPool) {
    let s = state(db.clone());
    let cam = service::create(&s, input("Front Door")).await.unwrap();
    let mut edit = input("Front Door");
    edit.onvif = None;
    service::update(&s, &cam.id, edit).await.unwrap();
    assert!(stored_secrets(&db, &cam.id).await.1.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn names_are_unique_ignoring_case(db: PgPool) {
    let s = state(db);
    service::create(&s, input("Front Door")).await.unwrap();
    assert!(service::create(&s, input("front door")).await.is_err());
    let other = service::create(&s, input("Garage")).await.unwrap();
    assert!(service::update(&s, &other.id, input("FRONT DOOR")).await.is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn ids_stay_unique_after_renames(db: PgPool) {
    let s = state(db);
    let first = service::create(&s, input("Front Door")).await.unwrap();
    service::update(&s, &first.id, input("Porch")).await.unwrap();
    let second = service::create(&s, input("Front Door")).await.unwrap();
    assert_eq!(second.id, "cam-front-door-2");
}

#[sqlx::test(migrations = "./migrations")]
async fn enable_disable_and_delete(db: PgPool) {
    let s = state(db);
    let cam = service::create(&s, input("Front Door")).await.unwrap();
    assert!(!service::set_enabled(&s, &cam.id, false).await.unwrap().enabled);
    service::delete(&s, &cam.id).await.unwrap();
    assert!(service::get(&s, &cam.id).await.is_err());
    assert!(service::delete(&s, &cam.id).await.is_err(), "deleting twice reports not found");
}

#[sqlx::test(migrations = "./migrations")]
async fn credentials_in_urls_are_moved_out_and_encrypted(db: PgPool) {
    let s = state(db.clone());
    let mut i = input("Ezviz");
    i.username = String::new();
    i.password = None;
    i.main_stream_url = "rtsp://admin:urlpass@192.168.1.26:554/Streaming/Channels/101".into();
    i.sub_stream_url = Some("rtsp://admin:urlpass@192.168.1.26:554/Streaming/Channels/102".into());
    let cam = service::create(&s, i).await.unwrap();

    assert_eq!(cam.main_stream.url, "rtsp://192.168.1.26:554/Streaming/Channels/101");
    assert_eq!(cam.sub_stream.as_ref().unwrap().url, "rtsp://192.168.1.26:554/Streaming/Channels/102");
    assert_eq!(cam.username, "admin");
    let row: (String, Option<String>) = sqlx::query_as("SELECT main_stream_url, sub_stream_url FROM cameras WHERE id = $1").bind(&cam.id).fetch_one(&db).await.unwrap();
    assert!(!row.0.contains("urlpass") && !row.1.unwrap().contains("urlpass"), "no password left in stored URLs");
    let (pw, _) = stored_secrets(&db, &cam.id).await;
    assert_eq!(s.credentials.open(&format!("camera:{}:password", cam.id), &pw.unwrap()).unwrap(), b"urlpass");
}

#[sqlx::test(migrations = "./migrations")]
async fn entered_password_wins_over_url_password(db: PgPool) {
    let s = state(db.clone());
    let mut i = input("Ezviz");
    i.main_stream_url = "rtsp://admin:urlpass@192.168.1.26/live".into();
    let cam = service::create(&s, i).await.unwrap();
    let (pw, _) = stored_secrets(&db, &cam.id).await;
    assert_eq!(s.credentials.open(&format!("camera:{}:password", cam.id), &pw.unwrap()).unwrap(), b"hunter2");
}

#[sqlx::test(migrations = "./migrations")]
async fn recording_needs_an_online_camera(db: PgPool) {
    let s = state(db);
    let cam = service::create(&s, input("Porch")).await.unwrap();
    // The inert supervisor never connects, so the camera stays offline.
    let err = service::start_recording(&s, &cam.id).await.unwrap_err();
    assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
    // Stopping when nothing records is a harmless no-op.
    let after = service::stop_recording(&s, &cam.id).await.unwrap();
    assert!(!after.recording_active);
}

#[sqlx::test(migrations = "./migrations")]
async fn cameras_show_their_newest_event(db: PgPool) {
    let s = state(db.clone());
    let a = service::create(&s, input("Front Door")).await.unwrap();
    let b = service::create(&s, input("Garage")).await.unwrap();
    for (id, at) in [("old", "2026-09-26T08:00:00Z"), ("new", "2026-09-26T09:00:00Z")] {
        sqlx::query("INSERT INTO events (id, camera_id, kind, start_time, end_time, source) VALUES ($1, $2, 'motion', $3::timestamptz, $3::timestamptz, 'test')")
            .bind(id)
            .bind(&a.id)
            .bind(at)
            .execute(&db)
            .await
            .unwrap();
    }
    let list = service::list(&s).await.unwrap();
    let last = |id: &str| list.iter().find(|c| c.id == id).unwrap().last_event.clone();
    assert_eq!(last(&a.id).map(|e| (e.event_id, e.kind)), Some(("new".to_string(), watchgrid_model::EventType::Motion)));
    assert_eq!(last(&b.id), None, "no events yet");
    assert_eq!(service::get(&s, &a.id).await.unwrap().last_event.map(|e| e.event_id), Some("new".to_string()));
}

#[sqlx::test(migrations = "./migrations")]
async fn a_new_camera_never_inherits_a_deleted_ones_history(db: PgPool) {
    let s = state(db.clone());
    let old = service::create(&s, input("Garage")).await.unwrap();
    sqlx::query("INSERT INTO events (camera_id, kind, start_time, end_time, source) VALUES ($1, 'motion', now(), now(), 'test')")
        .bind(&old.id)
        .execute(&db)
        .await
        .unwrap();
    service::delete(&s, &old.id).await.unwrap();

    let new = service::create(&s, input("Garage")).await.unwrap();
    assert_ne!(new.id, old.id, "the old id still names the old camera's events");
    assert_eq!(new.last_event, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn pre_record_is_kept_to_what_the_recorder_holds(db: PgPool) {
    let s = state(db);
    let mut i = input("Hall");
    i.recording.pre_record_seconds = 45;
    let cam = service::create(&s, i).await.unwrap();
    assert_eq!(cam.recording.pre_record_seconds, watchgrid_model::MAX_PRE_RECORD_SECONDS);
}
