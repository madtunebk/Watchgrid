//! Arming and disarming against a real PostgreSQL (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::{ArmSettings, CameraInput, MotionSettings, MotionSource, OnvifConfig, RecordingMode, RecordingSettings};

use crate::cameras;
use crate::credentials::CredentialStore;
use crate::state::AppState;

fn state(db: PgPool) -> AppState {
    AppState::inert(db, CredentialStore::from_key(&[5u8; 32]))
}

/// A camera with motion off (ONVIF chosen) and recording `mode`.
async fn camera(s: &AppState, name: &str, mode: RecordingMode) -> String {
    let input = CameraInput {
        name: name.into(),
        description: String::new(),
        location: String::new(),
        enabled: true,
        host: "192.168.1.30".into(),
        username: "admin".into(),
        password: None,
        main_stream_url: "rtsp://192.168.1.30:554/stream1".into(),
        sub_stream_url: None,
        onvif: Some(OnvifConfig { url: "http://192.168.1.30/onvif/device_service".into(), username: "admin".into(), password: None }),
        recording: RecordingSettings { mode, ..RecordingSettings::default() },
        motion: MotionSettings { enabled: false, source: MotionSource::Onvif, ..MotionSettings::default() },
    };
    cameras::create_camera(s, input).await.unwrap().id
}

async fn settings(s: &AppState, id: &str) -> ArmSettings {
    cameras::arm_settings(&s.db, id).await.unwrap().unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn arming_the_chosen_cameras_and_disarming_puts_them_back(db: PgPool) {
    let s = state(db);
    let hall = camera(&s, "Hall", RecordingMode::Manual).await;
    let yard = camera(&s, "Yard", RecordingMode::Continuous).await;
    let attic = camera(&s, "Attic", RecordingMode::Disabled).await;

    let armed = super::arm(&s, &[hall.clone(), yard.clone()]).await.unwrap();
    assert!(armed.armed && armed.since.is_some());
    assert_eq!(armed.cameras.len(), 2);
    assert_eq!(settings(&s, &hall).await, ArmSettings { motion_enabled: true, source: MotionSource::Software, mode: RecordingMode::Events });
    assert_eq!(settings(&s, &yard).await.mode, RecordingMode::Continuous, "already records: keeps its mode");
    assert!(!settings(&s, &attic).await.motion_enabled, "not chosen: untouched");

    // Arming more adds to them; arming again changes nothing.
    assert_eq!(super::arm(&s, std::slice::from_ref(&attic)).await.unwrap().cameras.len(), 3);
    assert_eq!(super::arm(&s, &[]).await.unwrap().cameras.len(), 3);

    let off = super::disarm(&s).await.unwrap();
    assert!(!off.armed && off.cameras.is_empty());
    assert_eq!(settings(&s, &hall).await, ArmSettings { motion_enabled: false, source: MotionSource::Onvif, mode: RecordingMode::Manual });
    assert_eq!(settings(&s, &attic).await.mode, RecordingMode::Disabled);
}

#[sqlx::test(migrations = "./migrations")]
async fn empty_means_every_camera_and_unknown_ones_are_refused(db: PgPool) {
    let s = state(db);
    camera(&s, "Hall", RecordingMode::Manual).await;
    camera(&s, "Yard", RecordingMode::Manual).await;
    assert!(super::arm(&s, &["cam-nope".into()]).await.is_err());
    assert!(!super::state(&s.db).await.unwrap().armed, "nothing armed after a refusal");
    assert_eq!(super::arm(&s, &[]).await.unwrap().cameras.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_change_made_while_armed_survives_disarm(db: PgPool) {
    let s = state(db);
    let hall = camera(&s, "Hall", RecordingMode::Manual).await;
    super::arm(&s, std::slice::from_ref(&hall)).await.unwrap();
    let mut now = settings(&s, &hall).await;
    now.mode = RecordingMode::Continuous;
    cameras::set_arm_settings(&s, &hall, now).await.unwrap();
    super::disarm(&s).await.unwrap();
    let back = settings(&s, &hall).await;
    assert_eq!(back.mode, RecordingMode::Continuous, "the hand-made change stays");
    assert!(!back.motion_enabled, "what arming set goes back");
}
