//! Recordings over HTTP — same functions as the mock's `recordings` module.

use super::client;
use crate::api::{ApiResult, Recording, RecordingQuery};

pub async fn list(query: &RecordingQuery) -> ApiResult<Vec<Recording>> {
    let mut params = Vec::new();
    if !query.camera_ids.is_empty() {
        params.push(format!("cameras={}", encode(&query.camera_ids.join(","))));
    }
    if let Some(from) = query.from {
        params.push(format!("from={}", encode(&from.to_rfc3339())));
    }
    if let Some(to) = query.to {
        params.push(format!("to={}", encode(&to.to_rfc3339())));
    }
    let qs = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
    client::get(&format!("/recordings{qs}")).await
}

/// Where the browser streams a recording's video from.
pub fn media_url(id: &str) -> Option<String> {
    Some(format!("/api/v1/recordings/{}/media", encode(id)))
}

fn encode(s: &str) -> String {
    js_sys::encode_uri_component(s).into()
}

pub async fn set_protected(id: &str, protected: bool) -> ApiResult<()> {
    client::put_no_content(&format!("/recordings/{}/protected", encode(id)), &serde_json::json!({ "protected": protected })).await
}
