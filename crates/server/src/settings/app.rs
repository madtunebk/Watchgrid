//! The Settings page document: defaults and validation.
//!
//! Applied immediately: the NVR name (server info), the time zone (event
//! "hours of day" filters, schedules), the session timeout, notifications and
//! the advanced options (`applied`). The listen address comes from the
//! environment (WATCHGRID_BIND); the Network section only shows it.

use sqlx::PgPool;
use watchgrid_model::{
    AdvancedSettings, AuthSettings, DateFormat, GeneralSettings, LogLevel, NetworkSettings, NotificationSettings, RecordingDefaults, RecordingMode,
    RtspTransport, Settings,
};

use super::store;
use crate::error::{ApiError, ApiResult};
use crate::timezone;

const KEY: &str = "app";

pub fn defaults(bind: std::net::SocketAddr) -> Settings {
    Settings {
        general: GeneralSettings {
            nvr_name: "Watchgrid".into(),
            timezone: timezone::name().into(),
            language: "en".into(),
            date_format: DateFormat::Iso,
            clock_24h: true,
        },
        recording: RecordingDefaults { mode: RecordingMode::Events, pre_record_seconds: 5, post_record_seconds: 10 },
        network: NetworkSettings { http_bind: bind.ip().to_string(), http_port: bind.port(), https_enabled: false, https_port: 8443 },
        auth: AuthSettings { enabled: false, session_timeout_minutes: 720 },
        notifications: NotificationSettings {
            camera_offline: true,
            person_detected: false,
            vehicle_detected: false,
            storage_low: true,
            recording_failed: true,
            camera_security: true,
            webhook_url: None,
        },
        advanced: AdvancedSettings { log_level: LogLevel::Info, rtsp_transport: RtspTransport::Tcp, reconnect_seconds: 2 },
    }
}

/// The configured time zone (Settings → General), else the machine's.
pub async fn timezone(db: &PgPool) -> String {
    match store::load::<Settings>(db, KEY).await {
        Ok(Some(s)) => s.general.timezone,
        _ => timezone::name().to_string(),
    }
}

/// Local (weekday 0 = Monday, minute of day) now, in the configured zone.
/// PostgreSQL does the zone arithmetic, so no time zone database is bundled.
pub async fn local_clock(db: &PgPool) -> sqlx::Result<(u8, u16)> {
    let tz = timezone(db).await;
    let (dow, minute): (i32, i32) = sqlx::query_as(
        "SELECT EXTRACT(ISODOW FROM now() AT TIME ZONE $1)::int - 1,
                (EXTRACT(HOUR FROM now() AT TIME ZONE $1) * 60 + EXTRACT(MINUTE FROM now() AT TIME ZONE $1))::int",
    )
    .bind(tz)
    .fetch_one(db)
    .await?;
    Ok((dow.clamp(0, 6) as u8, minute.clamp(0, 1439) as u16))
}

pub async fn load(db: &PgPool, bind: std::net::SocketAddr) -> ApiResult<Settings> {
    let mut s: Settings = store::load(db, KEY).await?.unwrap_or_else(|| defaults(bind));
    // The listen address comes from the environment; show the real one.
    s.network.http_bind = bind.ip().to_string();
    s.network.http_port = bind.port();
    Ok(s)
}

pub async fn save(db: &PgPool, mut s: Settings) -> ApiResult<Settings> {
    s.general.nvr_name = s.general.nvr_name.trim().to_string();
    s.notifications.webhook_url = s.notifications.webhook_url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    // Older settings may hold more pre-record than the recorder keeps.
    s.recording.pre_record_seconds = s.recording.pre_record_seconds.min(watchgrid_model::MAX_PRE_RECORD_SECONDS);
    validate(&s)?;
    // PostgreSQL's zone database decides which names are real.
    let known: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_timezone_names WHERE name = $1").bind(&s.general.timezone).fetch_optional(db).await?;
    if known.is_none() {
        return Err(ApiError::invalid(format!("Unknown time zone `{}`", s.general.timezone)));
    }
    store::save(db, KEY, &s).await?;
    tracing::info!("settings updated");
    Ok(s)
}

fn validate(s: &Settings) -> ApiResult<()> {
    let bad = |m: &str| Err(ApiError::invalid(m));
    if s.general.nvr_name.is_empty() {
        return bad("The NVR needs a name");
    }
    if s.general.nvr_name.chars().count() > 64 {
        return bad("The NVR name can be at most 64 characters");
    }
    if !timezone::valid(&s.general.timezone) {
        return bad("The time zone must be an IANA name like Europe/Bucharest");
    }
    if s.network.http_port == 0 || s.network.https_port == 0 {
        return bad("HTTP port must be between 1 and 65535");
    }
    if s.network.https_enabled && s.network.https_port == s.network.http_port {
        return bad("HTTP and HTTPS need different ports");
    }
    if s.network.http_bind.parse::<std::net::IpAddr>().is_err() {
        return bad("The bind address must be an IP address, e.g. 0.0.0.0");
    }
    if s.recording.post_record_seconds > watchgrid_model::MAX_POST_RECORD_SECONDS {
        return bad("Post-record is limited to 300 s");
    }
    if !(5..=10_080).contains(&s.auth.session_timeout_minutes) {
        return bad("Session timeout must be between 5 minutes and 7 days");
    }
    if !(1..=300).contains(&s.advanced.reconnect_seconds) {
        return bad("Reconnect delay must be between 1 and 300 seconds");
    }
    if let Some(url) = &s.notifications.webhook_url {
        let ok = url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host().is_some());
        if !ok {
            return bad("The webhook must be an http(s) URL");
        }
    }
    Ok(())
}
