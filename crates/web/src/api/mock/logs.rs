//! Mock log feed. Each read may append a fresh line so the viewer looks live.

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiResult, LogEntry, LogLevel, LogQuery};

const LIVE: &[(LogLevel, &str, &str)] = &[
    (LogLevel::Info, "recorder", "Driveway: motion detected"),
    (LogLevel::Info, "recorder", "Driveway: recording started (pre-record 5 s)"),
    (LogLevel::Info, "recorder", "Driveway: motion ended"),
    (LogLevel::Debug, "http", "GET /api/v1/cameras 200 (3 ms)"),
    (LogLevel::Info, "camera/garage", "Garage: reconnecting…"),
    (LogLevel::Warn, "camera/garage", "Garage: connection timed out after 5 s; retrying"),
    (LogLevel::Debug, "storage", "Disk usage 36.1 %, 2.3 TB free"),
];

fn append_live() {
    if js_sys::Math::random() > 0.45 {
        return;
    }
    with_db(|db| {
        let (level, source, message) = LIVE[(js_sys::Math::random() * LIVE.len() as f64) as usize % LIVE.len()];
        let n = db.logs.len();
        db.logs.push(LogEntry { id: format!("log-{n:05}"), time: Utc::now(), level, source: source.into(), message: message.into() });
    });
}

pub async fn list(query: &LogQuery) -> ApiResult<Vec<LogEntry>> {
    latency().await;
    append_live();
    let needle = query.search.as_deref().unwrap_or("").to_lowercase();
    Ok(with_db(|db| {
        let matching: Vec<&LogEntry> = db
            .logs
            .iter()
            .filter(|l| query.min_level.is_none_or(|m| l.level >= m))
            .filter(|l| needle.is_empty() || l.message.to_lowercase().contains(&needle) || l.source.to_lowercase().contains(&needle))
            .collect();
        let limit = query.limit.map_or(usize::MAX, |l| l as usize);
        matching.into_iter().rev().take(limit).rev().cloned().collect()
    }))
}
