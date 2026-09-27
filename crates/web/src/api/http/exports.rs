//! Export destinations and jobs over HTTP — same functions as the mock's
//! `exports` module. Secrets are sent when adding or replacing them, never returned.

use super::client;
use crate::api::{ApiResult, AutoUpload, ConnectionProbe, ExportJob, ExportTarget, ExportTargetInput, ExportTargetSettings};

fn enc(s: &str) -> String {
    js_sys::encode_uri_component(s).into()
}

pub async fn targets() -> ApiResult<Vec<ExportTarget>> {
    client::get("/exports/targets").await
}

pub async fn start(event_id: &str, target_id: &str) -> ApiResult<ExportJob> {
    client::post(&format!("/events/{}/export", enc(event_id)), Some(&serde_json::json!({ "targetId": target_id }))).await
}

pub async fn start_recording(recording_id: &str, target_id: &str) -> ApiResult<ExportJob> {
    client::post(&format!("/recordings/{}/export", enc(recording_id)), Some(&serde_json::json!({ "targetId": target_id }))).await
}

pub async fn job(id: &str) -> ApiResult<ExportJob> {
    client::get(&format!("/exports/jobs/{}", enc(id))).await
}

pub async fn pending() -> ApiResult<Vec<ExportJob>> {
    client::get("/exports/jobs").await
}

pub async fn cancel(id: &str) -> ApiResult<ExportJob> {
    client::post(&format!("/exports/jobs/{}/cancel", enc(id)), None::<&()>).await
}

pub async fn test(input: &ExportTargetInput) -> ApiResult<ConnectionProbe> {
    client::post("/exports/targets/test", Some(input)).await
}

pub async fn create(input: ExportTargetInput) -> ApiResult<ExportTarget> {
    client::post("/exports/targets", Some(&input)).await
}

pub async fn settings(id: &str) -> ApiResult<ExportTargetSettings> {
    client::get(&format!("/exports/targets/{}", enc(id))).await
}

pub async fn test_saved(id: &str, input: &ExportTargetInput) -> ApiResult<ConnectionProbe> {
    client::post(&format!("/exports/targets/{}/test", enc(id)), Some(input)).await
}

pub async fn update(id: &str, input: ExportTargetInput) -> ApiResult<ExportTarget> {
    client::put(&format!("/exports/targets/{}", enc(id)), &input).await
}

pub async fn set_auto(id: &str, rule: AutoUpload) -> ApiResult<()> {
    client::put_no_content(&format!("/exports/targets/{}/auto-upload", enc(id)), &serde_json::json!({ "rule": rule })).await
}

pub async fn check(id: &str) -> ApiResult<ConnectionProbe> {
    client::post(&format!("/exports/targets/{}/check", enc(id)), None::<&()>).await
}

/// Tests the destination again (clears or records its problem).
pub async fn reconnect(id: &str) -> ApiResult<()> {
    client::post::<ExportTarget>(&format!("/exports/targets/{}/reconnect", enc(id)), None::<&()>).await.map(|_| ())
}

pub async fn delete(id: &str) -> ApiResult<()> {
    client::delete(&format!("/exports/targets/{}", enc(id))).await
}
