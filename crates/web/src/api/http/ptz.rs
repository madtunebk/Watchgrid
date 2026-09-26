//! PTZ over HTTP — same functions as the mock's `ptz` module.

use super::client;
use crate::api::{ApiResult, PtzMove, PtzPreset, PtzState};

fn enc(s: &str) -> String {
    js_sys::encode_uri_component(s).into()
}

pub async fn state(id: &str) -> ApiResult<PtzState> {
    client::get(&format!("/cameras/{}/ptz", enc(id))).await
}

pub async fn move_at(id: &str, m: PtzMove) -> ApiResult<()> {
    client::post_json_no_content(&format!("/cameras/{}/ptz/move", enc(id)), &m).await
}

pub async fn stop(id: &str) -> ApiResult<()> {
    client::post_no_content(&format!("/cameras/{}/ptz/stop", enc(id))).await
}

pub async fn goto(id: &str, token: &str) -> ApiResult<()> {
    client::post_no_content(&format!("/cameras/{}/ptz/presets/{}/goto", enc(id), enc(token))).await
}

pub async fn save(id: &str, name: &str) -> ApiResult<PtzPreset> {
    client::post(&format!("/cameras/{}/ptz/presets", enc(id)), Some(&serde_json::json!({ "name": name }))).await
}

pub async fn remove(id: &str, token: &str) -> ApiResult<()> {
    client::delete(&format!("/cameras/{}/ptz/presets/{}", enc(id), enc(token))).await
}
