use crate::api::{
    AdvancedSettings, AuthSettings, DateFormat, GeneralSettings, LogLevel, NetworkSettings, NotificationSettings,
    RecordingDefaults, RecordingMode, RtspTransport, Settings,
};

pub fn defaults(nvr_name: &str) -> Settings {
    Settings {
        general: GeneralSettings {
            nvr_name: nvr_name.into(),
            timezone: "Europe/Bucharest".into(),
            language: "en".into(),
            date_format: DateFormat::Iso,
            clock_24h: true,
        },
        recording: RecordingDefaults { mode: RecordingMode::Events, pre_record_seconds: 5, post_record_seconds: 15 },
        network: NetworkSettings { http_bind: "0.0.0.0".into(), http_port: 8080, https_enabled: false, https_port: 8443 },
        auth: AuthSettings { enabled: false, session_timeout_minutes: 720 },
        notifications: NotificationSettings {
            camera_offline: true,
            person_detected: true,
            vehicle_detected: false,
            storage_low: true,
            recording_failed: true,
            webhook_url: None,
        },
        advanced: AdvancedSettings {
            log_level: LogLevel::Info,
            rtsp_transport: RtspTransport::Tcp,
            reconnect_seconds: 5,
            hardware_decoding: true,
        },
    }
}
