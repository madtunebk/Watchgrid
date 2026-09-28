//! Server-side validation of camera input. The UI validates too, for quick
//! feedback; this is the authoritative check.

use watchgrid_model::CameraInput;

use crate::error::ApiError;

const MAX_NAME: usize = 80;
const MAX_LOCATION: usize = 80;
const MAX_DESCRIPTION: usize = 500;
/// Usernames and passwords: kept exactly as typed, but within reason.
const MAX_SECRET: usize = 256;

/// A real URL with this scheme and a host (not just the right prefix).
fn url_with(url: &str, schemes: &[&str]) -> bool {
    url::Url::parse(url.trim()).is_ok_and(|u| schemes.contains(&u.scheme()) && u.host_str().is_some_and(|h| !h.is_empty()))
}

/// An IPv4 or IPv6 address, or a host name by the network rules (RFC 1123:
/// labels of letters, digits and `-`, joined by dots). `192.168.1.999` is
/// neither: a name's last label is never all digits.
fn host_ok(host: &str) -> bool {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let label = |l: &str| (1..=63).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') && !l.starts_with('-') && !l.ends_with('-');
    let labels: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    host.len() <= 253 && labels.iter().all(|l| label(l)) && labels.last().is_some_and(|l| !l.bytes().all(|b| b.is_ascii_digit()))
}

/// A login field as typed: no length or control-character surprises.
fn login_ok(s: &str) -> bool {
    s.chars().count() <= MAX_SECRET && !crate::text::has_control(s)
}

pub fn check(i: &CameraInput) -> Result<(), ApiError> {
    let invalid = |m: &str| Err(ApiError::invalid(m));
    let name = i.name.trim();
    if name.is_empty() {
        return invalid("Camera name is required");
    }
    if name.chars().count() > MAX_NAME {
        return invalid("Camera name is too long (80 characters at most)");
    }
    if i.location.chars().count() > MAX_LOCATION {
        return invalid("Location is too long (80 characters at most)");
    }
    if i.description.chars().count() > MAX_DESCRIPTION {
        return invalid("Description is too long (500 characters at most)");
    }
    let host = i.host.trim();
    if host.is_empty() {
        return invalid("Host or IP address is required");
    }
    if host.contains("://") {
        return invalid("Host must be a host name or IP address, without a scheme");
    }
    if !host_ok(host) {
        return invalid("Host must be an IP address or a host name, e.g. 192.168.1.20 or camera.local");
    }
    if !url_with(&i.main_stream_url, &["rtsp", "rtsps"]) {
        return invalid("Main stream URL must be an rtsp:// or rtsps:// address with a host, e.g. rtsp://192.168.1.20:554/stream1");
    }
    if i.sub_stream_url.as_deref().is_some_and(|u| !url_with(u, &["rtsp", "rtsps"])) {
        return invalid("Substream URL must be an rtsp:// or rtsps:// address with a host");
    }
    if let Some(o) = &i.onvif {
        if o.url.trim().starts_with("https://") {
            return invalid("ONVIF over https:// isn't supported yet: use the camera's http:// ONVIF address");
        }
        if !url_with(&o.url, &["http"]) {
            return invalid("ONVIF URL must be an http:// address with a host, e.g. http://192.168.1.20/onvif/device_service");
        }
    }
    let logins = [Some(i.username.as_str()), i.password.as_deref(), i.onvif.as_ref().map(|o| o.username.as_str()), i.onvif.as_ref().and_then(|o| o.password.as_deref())];
    if logins.into_iter().flatten().any(|s| !login_ok(s)) {
        return invalid("Usernames and passwords can be at most 256 characters, without control characters");
    }
    let r = &i.recording;
    if r.post_record_seconds > watchgrid_model::MAX_POST_RECORD_SECONDS {
        return invalid("Post-record is limited to 300 s");
    }
    if !(30..=3600).contains(&r.max_clip_seconds) {
        return invalid("Maximum clip duration must be between 30 and 3600 seconds");
    }
    if r.schedule.len() > 20 {
        return invalid("Use at most 20 schedule windows");
    }
    for w in &r.schedule {
        if w.days.is_empty() || w.days.iter().any(|d| *d > 6) {
            return invalid("Each schedule window needs at least one day");
        }
        if w.start_minute >= 1440 || w.end_minute >= 1440 {
            return invalid("Schedule times must be between 00:00 and 23:59");
        }
    }
    if r.mode == watchgrid_model::RecordingMode::Scheduled && r.schedule.is_empty() {
        return invalid("Scheduled recording needs at least one time window");
    }
    if i.motion.sensitivity > 100 {
        return invalid("Motion sensitivity must be between 0 and 100");
    }
    if i.motion.zones.len() > 16 {
        return invalid("At most 16 motion zones");
    }
    for z in &i.motion.zones {
        let inside = |v: f32| (0.0..=1.0).contains(&v);
        if z.name.trim().is_empty() || z.name.chars().count() > 40 {
            return invalid("Each motion zone needs a name of at most 40 characters");
        }
        if !(inside(z.x) && inside(z.y) && z.w > 0.0 && z.h > 0.0 && z.x + z.w <= 1.001 && z.y + z.h <= 1.001) {
            return invalid(&format!("Motion zone `{}` must lie inside the picture", z.name.trim()));
        }
    }
    if i.recording.retention_days.is_some_and(|d| !(1..=3650).contains(&d)) {
        return invalid("Keep recordings between 1 and 3650 days, or without a camera limit");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use watchgrid_model::{MotionSettings, OnvifConfig, RecordingSettings};

    use super::*;

    fn valid() -> CameraInput {
        CameraInput {
            name: "Front Door".into(),
            description: String::new(),
            location: String::new(),
            enabled: true,
            host: "192.168.1.26".into(),
            username: "admin".into(),
            password: None,
            main_stream_url: "rtsp://192.168.1.26:554/stream1".into(),
            sub_stream_url: None,
            onvif: None,
            recording: RecordingSettings::default(),
            motion: MotionSettings::default(),
        }
    }

    fn rejects(f: impl FnOnce(&mut CameraInput)) -> bool {
        let mut i = valid();
        f(&mut i);
        check(&i).is_err()
    }

    #[test]
    fn accepts_a_single_stream_camera() {
        assert!(check(&valid()).is_ok());
    }

    #[test]
    fn requires_name_and_host() {
        assert!(rejects(|i| i.name = "   ".into()));
        assert!(rejects(|i| i.host = String::new()));
        assert!(rejects(|i| i.name = "x".repeat(81)));
    }

    #[test]
    fn host_is_not_a_url() {
        assert!(rejects(|i| i.host = "rtsp://192.168.1.26".into()));
        assert!(rejects(|i| i.host = "192.168.1.26 x".into()));
    }

    #[test]
    fn stream_urls_must_be_rtsp() {
        assert!(rejects(|i| i.main_stream_url = "http://cam/stream".into()));
        assert!(rejects(|i| i.sub_stream_url = Some("ftp://cam".into())));
        assert!(!rejects(|i| i.sub_stream_url = Some("rtsps://cam:322/sub".into())));
    }

    #[test]
    fn onvif_url_must_be_http() {
        assert!(rejects(|i| i.onvif = Some(OnvifConfig { url: "cam/onvif".into(), username: "a".into(), password: None })));
    }

    #[test]
    fn recording_limits() {
        assert!(rejects(|i| i.recording.post_record_seconds = 301));
        assert!(!rejects(|i| i.recording.pre_record_seconds = 45), "more pre-record is brought down to 30 by the service, not refused");
        assert!(rejects(|i| i.recording.max_clip_seconds = 10));
        assert!(rejects(|i| i.recording.mode = watchgrid_model::RecordingMode::Scheduled), "scheduled without windows");
        let window = |days: Vec<u8>, start, end| watchgrid_model::ScheduleWindow { days, start_minute: start, end_minute: end };
        assert!(rejects(|i| i.recording.schedule = vec![window(vec![], 0, 60)]));
        assert!(rejects(|i| i.recording.schedule = vec![window(vec![7], 0, 60)]));
        assert!(rejects(|i| i.recording.schedule = vec![window(vec![0], 0, 1440)]));
        assert!(!rejects(|i| {
            i.recording.mode = watchgrid_model::RecordingMode::Scheduled;
            i.recording.schedule = vec![window(vec![0, 1], 22 * 60, 6 * 60)];
        }));
        assert!(rejects(|i| i.motion.sensitivity = 101));
        assert!(rejects(|i| i.recording.retention_days = Some(0)));
        let zone = |x: f32, w: f32| watchgrid_model::MotionZone { id: "z".into(), name: "Door".into(), x, y: 0.1, w, h: 0.2, exclude: false };
        assert!(!rejects(|i| i.motion.zones = vec![zone(0.5, 0.5)]));
        assert!(rejects(|i| i.motion.zones = vec![zone(0.7, 0.5)]), "past the right edge");
        assert!(rejects(|i| i.motion.zones = vec![zone(0.1, 0.0)]), "empty");
        assert!(rejects(|i| i.motion.zones = vec![zone(0.1, 0.1); 17]));
    }

    #[test]
    fn hosts_are_real_addresses_or_names() {
        for good in ["192.168.1.26", "camera.local", "cam-1", "fe80::1", "::1"] {
            assert!(host_ok(good), "{good}");
        }
        for bad in ["192.168.1.999", "cam<1>", "a b", "cam!", "-cam", "cam_1", ""] {
            assert!(!host_ok(bad), "{bad}");
        }
    }

    #[test]
    fn urls_need_the_scheme_and_a_host() {
        assert!(url_with("rtsp://192.168.1.26:554/Streaming/Channels/101?x=1&y=2", &["rtsp", "rtsps"]));
        assert!(!url_with("rtsp://", &["rtsp", "rtsps"]));
        assert!(!url_with("rtsp:stream1", &["rtsp", "rtsps"]));
        assert!(!url_with("http://192.168.1.26/", &["rtsp", "rtsps"]));
        let onvif = |url: &str| Some(watchgrid_model::OnvifConfig { url: url.into(), username: "admin".into(), password: None });
        assert!(rejects(|i| i.onvif = onvif("https://192.168.1.26/onvif/device_service")), "the ONVIF client speaks http only");
        assert!(!rejects(|i| i.onvif = onvif("http://192.168.1.26:2020/onvif/device_service")));
    }

    #[test]
    fn logins_are_kept_as_typed_but_bounded() {
        assert!(login_ok("p@ss'w\"ord; DROP TABLE x; --"), "quotes are just characters");
        assert!(!login_ok("pass\0word"));
        assert!(!login_ok(&"x".repeat(257)));
        assert!(rejects(|i| i.password = Some("a\u{7}b".into())));
    }

    #[test]
    fn free_text_is_bounded() {
        assert!(rejects(|i| i.description = "x".repeat(501)));
        assert!(rejects(|i| i.location = "x".repeat(81)));
    }
}
