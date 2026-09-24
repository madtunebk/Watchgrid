//! Reconnect delays: start small, double on each failure, cap at a minute.

use std::time::Duration;

pub const BASE: Duration = Duration::from_secs(2);
pub const MAX: Duration = Duration::from_secs(60);

/// Delay before reconnect attempt number `attempt` (1-based).
pub fn delay(attempt: u32) -> Duration {
    let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(16));
    BASE.saturating_mul(factor).min(MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_then_caps() {
        assert_eq!(delay(1), Duration::from_secs(2));
        assert_eq!(delay(2), Duration::from_secs(4));
        assert_eq!(delay(3), Duration::from_secs(8));
        assert_eq!(delay(6), MAX);
        assert_eq!(delay(1000), MAX);
    }
}
