//! Failed-login throttling per client address (memory only).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(600);
const MAX_FAILURES: usize = 10;

#[derive(Default)]
pub struct LoginLimiter(Mutex<HashMap<IpAddr, Vec<Instant>>>);

impl LoginLimiter {
    /// Too many recent failures from this address?
    pub fn blocked(&self, ip: IpAddr) -> bool {
        let mut map = self.0.lock().expect("limiter lock");
        let list = map.entry(ip).or_default();
        list.retain(|t| t.elapsed() < WINDOW);
        list.len() >= MAX_FAILURES
    }

    pub fn failed(&self, ip: IpAddr) {
        self.0.lock().expect("limiter lock").entry(ip).or_default().push(Instant::now());
    }

    pub fn succeeded(&self, ip: IpAddr) {
        self.0.lock().expect("limiter lock").remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_repeated_failures_and_resets_on_success() {
        let l = LoginLimiter::default();
        let ip: IpAddr = "10.0.0.5".parse().unwrap();
        for _ in 0..MAX_FAILURES {
            assert!(!l.blocked(ip));
            l.failed(ip);
        }
        assert!(l.blocked(ip));
        assert!(!l.blocked("10.0.0.6".parse().unwrap()), "per address");
        l.succeeded(ip);
        assert!(!l.blocked(ip));
    }
}
