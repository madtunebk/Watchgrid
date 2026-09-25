//! Human-readable formatting for sizes, rates, durations and times.

use chrono::{DateTime, Local, Utc};

/// "just now", "4 min ago", "3 h ago", "2 d ago" (also handles the future).
pub fn relative(t: DateTime<Utc>) -> String {
    let secs = (Utc::now() - t).num_seconds();
    let (abs, future) = (secs.unsigned_abs(), secs < 0);
    let v = match abs {
        0..45 => return "just now".into(),
        45..3_600 => format!("{} min", (abs + 30) / 60),
        3_600..86_400 => format!("{} h", (abs + 1_800) / 3_600),
        _ => format!("{} d", (abs + 43_200) / 86_400),
    };
    if future { format!("in {v}") } else { format!("{v} ago") }
}

/// "3d 05h 17m" / "05h 17m"
pub fn uptime(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3_600, secs % 3_600 / 60);
    if d > 0 { format!("{d}d {h:02}h {m:02}m") } else { format!("{h:02}h {m:02}m") }
}

/// Clip duration: "0:23", "3:17", "1:02:05"
pub fn duration(secs: u32) -> String {
    let (h, m, s) = (secs / 3_600, secs % 3_600 / 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if n == 0 {
        return "0 B".into();
    }
    let i = ((n as f64).log(1024.0).floor() as usize).min(UNITS.len() - 1);
    let v = n as f64 / 1024f64.powi(i as i32);
    match i {
        0 => format!("{n} B"),
        _ if v >= 100.0 => format!("{v:.0} {}", UNITS[i]),
        _ => format!("{v:.1} {}", UNITS[i]),
    }
}

/// Bytes/s → "3.4 Mb/s"
pub fn rate(bytes_per_sec: u64) -> String {
    let bits = bytes_per_sec as f64 * 8.0;
    if bits >= 1e6 {
        format!("{:.1} Mb/s", bits / 1e6)
    } else if bits >= 1e3 {
        format!("{:.0} kb/s", bits / 1e3)
    } else {
        format!("{bits:.0} b/s")
    }
}

pub fn clock(t: DateTime<Local>) -> String {
    t.format("%H:%M:%S").to_string()
}

/// Local wall-clock time of an event: "06:14"
pub fn time_of_day(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&Local).format("%H:%M").to_string()
}

/// "Chrome 153 · Linux" from a User-Agent string (full string on hover).
pub fn client(user_agent: &str) -> String {
    let ua = user_agent;
    let version = |name: &str| ua.split(name).nth(1).and_then(|r| r.split(['.', ' ', ';']).next()).filter(|v| !v.is_empty()).map(|v| format!("{} {v}", name.trim_end_matches('/')));
    let browser = version("Edg/").map(|b| b.replace("Edg", "Edge"))
        .or_else(|| version("Firefox/"))
        .or_else(|| version("Chrome/"))
        .or_else(|| ua.contains("Safari/").then(|| version("Version/").map(|v| v.replace("Version", "Safari")).unwrap_or_else(|| "Safari".into())))
        .unwrap_or_else(|| ua.split('/').next().unwrap_or("Unknown").to_string());
    let os = if ua.contains("Android") { "Android" } else if ua.contains("iPhone") || ua.contains("iPad") { "iOS" } else if ua.contains("Windows") { "Windows" } else if ua.contains("Mac OS") { "macOS" } else if ua.contains("Linux") { "Linux" } else { "" };
    if os.is_empty() { browser } else { format!("{browser} · {os}") }
}
