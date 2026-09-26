//! Shared HTTP(S) client for outside services (exports, webhooks): rustls
//! with bundled roots (never the system's), one connection pool.

use std::sync::OnceLock;
use std::time::Duration;

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent("Watchgrid")
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client")
    })
}
