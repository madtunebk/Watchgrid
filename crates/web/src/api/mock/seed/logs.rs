//! A plausible server log leading up to "now".

use chrono::{Duration, Utc};

use crate::api::{Camera, CameraStatus, LogEntry, LogLevel};

fn entry(n: usize, minutes_ago: f64, level: LogLevel, source: &str, message: String) -> LogEntry {
    LogEntry {
        id: format!("log-{n:05}"),
        time: Utc::now() - Duration::milliseconds((minutes_ago * 60_000.0) as i64),
        level,
        source: source.into(),
        message,
    }
}

/// Startup lines only (fresh install).
pub fn boot() -> Vec<LogEntry> {
    vec![
        entry(0, 180.0, LogLevel::Info, "server", "Watchgrid 0.1.0-dev starting".into()),
        entry(1, 179.9, LogLevel::Info, "storage", "Recording volume /volume1/nvr mounted (3.6 TB)".into()),
        entry(2, 179.9, LogLevel::Info, "http", "Listening on 0.0.0.0:8080".into()),
    ]
}

pub fn history(cameras: &[Camera]) -> Vec<LogEntry> {
    let mut out = boot();
    let mut n = out.len();
    let mut push = |minutes: f64, level, source: &str, message: String| {
        out.push(entry(n, minutes, level, source, message));
        n += 1;
    };
    for (i, c) in cameras.iter().enumerate() {
        let src = format!("camera/{}", c.id.trim_start_matches("cam-"));
        push(179.0 - i as f64 * 0.1, LogLevel::Info, &src, format!("{} connecting to {}", c.name, c.host));
        if c.status == CameraStatus::Online {
            push(178.9 - i as f64 * 0.1, LogLevel::Info, &src, format!("{} connected (H.264 1920x1080 25 fps)", c.name));
        } else {
            push(178.9 - i as f64 * 0.1, LogLevel::Warn, &src, format!("{} connection timed out after 5 s; retrying", c.name));
        }
    }
    push(95.0, LogLevel::Info, "recorder", "Front Door: motion detected".into());
    push(94.9, LogLevel::Info, "recorder", "Front Door: recording started (pre-record 5 s)".into());
    push(94.2, LogLevel::Info, "recorder", "Front Door: motion ended".into());
    push(93.9, LogLevel::Info, "recorder", "Front Door: recording stopped, clip 1:06, 31.4 MB".into());
    push(60.0, LogLevel::Debug, "retention", "Retention pass: nothing to delete (oldest 7 d, limit 14 d)".into());
    push(12.0, LogLevel::Warn, "camera/garage", "Garage: RTSP connection lost (connection reset by peer)".into());
    push(4.0, LogLevel::Error, "camera/garage", "Garage: reconnect failed 3 times; backing off 30 s".into());
    push(2.0, LogLevel::Info, "recorder", "Front Door: person detected (0.82) via ONVIF".into());
    out
}
