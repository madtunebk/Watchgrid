//! Events over HTTP — same functions as the mock's `events` module.

use super::client;
use crate::api::{ApiResult, EventDetail, EventPage, EventQuery};

pub async fn list(q: &EventQuery) -> ApiResult<EventPage> {
    let mut params = Vec::new();
    let mut add = |k: &str, v: String| params.push(format!("{k}={}", String::from(js_sys::encode_uri_component(&v))));
    if let Some(c) = &q.camera_id {
        add("camera", c.clone());
    }
    if !q.kinds.is_empty() {
        let names: Vec<String> = q.kinds.iter().filter_map(|k| serde_json::to_value(k).ok()?.as_str().map(String::from)).collect();
        add("kinds", names.join(","));
    }
    if let Some(from) = q.from {
        add("from", from.to_rfc3339());
    }
    if let Some(to) = q.to {
        add("to", to.to_rfc3339());
    }
    if let Some((a, b)) = q.hours {
        add("hours", format!("{a}-{b}"));
    }
    if let Some(d) = q.min_duration {
        add("minDuration", d.to_string());
    }
    if q.protected_only {
        add("protectedOnly", "true".into());
    }
    if let Some(l) = q.limit {
        add("limit", l.to_string());
    }
    if let Some(o) = q.offset {
        add("offset", o.to_string());
    }
    let qs = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
    client::get(&format!("/events{qs}")).await
}

pub async fn get(id: &str) -> ApiResult<EventDetail> {
    client::get(&format!("/events/{}", String::from(js_sys::encode_uri_component(id)))).await
}

pub async fn set_protected(id: &str, protected: bool) -> ApiResult<()> {
    let path = format!("/events/{}/protected", String::from(js_sys::encode_uri_component(id)));
    client::put_no_content(&path, &serde_json::json!({ "protected": protected })).await
}

pub async fn delete(id: &str) -> ApiResult<()> {
    client::delete(&format!("/events/{}", String::from(js_sys::encode_uri_component(id)))).await
}
