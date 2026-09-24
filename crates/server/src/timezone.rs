//! The server's time zone (IANA name), for "hours of day" filters.
//! From `TZ`, else `/etc/timezone`, else the `/etc/localtime` link; UTC otherwise.

use std::sync::OnceLock;

pub fn name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| detect().filter(|n| valid(n)).unwrap_or_else(|| "UTC".into()))
}

fn detect() -> Option<String> {
    if let Ok(tz) = std::env::var("TZ") {
        return Some(tz.trim_start_matches(':').to_string());
    }
    if let Ok(s) = std::fs::read_to_string("/etc/timezone") {
        return Some(s.trim().to_string());
    }
    let link = std::fs::read_link("/etc/localtime").ok()?;
    let s = link.to_string_lossy();
    s.split_once("zoneinfo/").map(|(_, name)| name.to_string())
}

/// IANA names only: letters, digits and `/_+-`.
fn valid(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::valid;

    #[test]
    fn accepts_iana_names_only() {
        assert!(valid("Europe/Bucharest") && valid("UTC") && valid("Etc/GMT+2"));
        assert!(!valid("") && !valid("UTC'; DROP TABLE events; --"));
    }
}
