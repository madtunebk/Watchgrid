//! Server-side validation of camera input. The UI validates too, for quick
//! feedback; this is the authoritative check.

use watchgrid_model::CameraInput;

use crate::error::ApiError;

const MAX_NAME: usize = 80;

fn rtsp(url: &str) -> bool {
    url.starts_with("rtsp://") || url.starts_with("rtsps://")
}

fn http(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
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
    let host = i.host.trim();
    if host.is_empty() {
        return invalid("Host or IP address is required");
    }
    if host.contains(char::is_whitespace) || host.contains("://") {
        return invalid("Host must be a host name or IP address, without a scheme");
    }
    if !rtsp(i.main_stream_url.trim()) {
        return invalid("Main stream URL must start with rtsp:// or rtsps://");
    }
    if i.sub_stream_url.as_deref().is_some_and(|u| !rtsp(u.trim())) {
        return invalid("Substream URL must start with rtsp:// or rtsps://");
    }
    if i.onvif.as_ref().is_some_and(|o| !http(o.url.trim())) {
        return invalid("ONVIF URL must start with http:// or https://");
    }
    let r = &i.recording;
    if r.pre_record_seconds > 60 || r.post_record_seconds > 300 {
        return invalid("Pre-record is limited to 60 s and post-record to 300 s");
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
        assert!(rejects(|i| i.recording.pre_record_seconds = 61));
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
    }
}
