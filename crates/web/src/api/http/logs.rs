//! Server logs over HTTP — same functions as the mock's `logs` module.

use super::client;
use crate::api::{ApiResult, LogEntry, LogQuery};

pub async fn list(q: &LogQuery) -> ApiResult<Vec<LogEntry>> {
    let mut params = Vec::new();
    if let Some(level) = q.min_level.and_then(|l| serde_json::to_value(l).ok()).and_then(|v| v.as_str().map(String::from)) {
        params.push(format!("minLevel={level}"));
    }
    if let Some(s) = q.search.as_deref().filter(|s| !s.is_empty()) {
        params.push(format!("search={}", String::from(js_sys::encode_uri_component(s))));
    }
    if let Some(l) = q.limit {
        params.push(format!("limit={l}"));
    }
    let qs = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
    client::get(&format!("/system/logs{qs}")).await
}
