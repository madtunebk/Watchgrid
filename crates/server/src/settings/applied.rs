//! Settings → Advanced as the running process sees them: RTSP transport,
//! reconnect delay and log level. Stored settings are applied at start and
//! after every save; readers get plain values without touching the database.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tracing_subscriber::{EnvFilter, Registry, reload};
use watchgrid_model::{AdvancedSettings, LogLevel, RtspTransport};

static UDP: AtomicBool = AtomicBool::new(false);
static RECONNECT_SECS: AtomicU32 = AtomicU32::new(2);
static LOG_FILTER: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();
static LOG_FROM_ENV: AtomicBool = AtomicBool::new(false);

/// `RUST_LOG` decides the log level (Settings can't change it).
pub fn log_level_from_env() -> bool {
    LOG_FROM_ENV.load(Ordering::Relaxed)
}

/// Open camera streams over UDP instead of TCP.
pub fn rtsp_udp() -> bool {
    UDP.load(Ordering::Relaxed)
}

/// First reconnect delay; later attempts double it.
pub fn reconnect_base() -> Duration {
    Duration::from_secs(u64::from(RECONNECT_SECS.load(Ordering::Relaxed).max(1)))
}

/// The log filter to start with, and whether Settings may change it
/// (`RUST_LOG` set by the admin wins over the Settings page).
pub fn initial_log_filter() -> (EnvFilter, bool) {
    match EnvFilter::try_from_default_env() {
        Ok(filter) => {
            LOG_FROM_ENV.store(true, Ordering::Relaxed);
            (filter, false)
        }
        Err(_) => (log_filter(LogLevel::Info), true),
    }
}

/// Let `apply` change the log level through this handle.
pub fn manage_log_level(handle: reload::Handle<EnvFilter, Registry>) {
    let _ = LOG_FILTER.set(handle);
}

fn log_filter(level: LogLevel) -> EnvFilter {
    let level = match level {
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    };
    EnvFilter::new(format!("watchgrid={level}"))
}

/// Apply the advanced settings; returns whether open camera streams must
/// reconnect (the transport changed).
pub fn apply(a: &AdvancedSettings) -> bool {
    let udp = a.rtsp_transport == RtspTransport::Udp;
    let changed = UDP.swap(udp, Ordering::Relaxed) != udp;
    RECONNECT_SECS.store(a.reconnect_seconds.clamp(1, 300), Ordering::Relaxed);
    if let Some(handle) = LOG_FILTER.get() {
        if let Err(e) = handle.reload(log_filter(a.log_level)) {
            tracing::warn!("cannot change the log level: {e}");
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_transport_and_reconnect_delay() {
        let mut a = AdvancedSettings { log_level: LogLevel::Info, rtsp_transport: RtspTransport::Udp, reconnect_seconds: 7 };
        assert!(apply(&a), "TCP → UDP reconnects the streams");
        assert!(rtsp_udp());
        assert_eq!(reconnect_base(), Duration::from_secs(7));
        assert!(!apply(&a), "unchanged transport: no reconnect");
        a.rtsp_transport = RtspTransport::Tcp;
        a.reconnect_seconds = 2;
        assert!(apply(&a));
        assert!(!rtsp_udp());
    }
}
