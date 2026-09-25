//! Reconnect delays: start at Settings → Advanced → Reconnect delay, double
//! on each failure, cap at a minute (or the setting, if that is longer).

use std::time::Duration;

use crate::settings::applied;

pub const MAX: Duration = Duration::from_secs(60);

/// Delay before reconnect attempt number `attempt` (1-based).
pub fn delay(attempt: u32) -> Duration {
    delay_from(applied::reconnect_base(), attempt)
}

fn delay_from(base: Duration, attempt: u32) -> Duration {
    let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(16));
    base.saturating_mul(factor).min(MAX.max(base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_then_caps() {
        let base = Duration::from_secs(2);
        assert_eq!(delay_from(base, 1), Duration::from_secs(2));
        assert_eq!(delay_from(base, 2), Duration::from_secs(4));
        assert_eq!(delay_from(base, 3), Duration::from_secs(8));
        assert_eq!(delay_from(base, 6), MAX);
        assert_eq!(delay_from(base, 1000), MAX);
        // A setting above the cap is used as is.
        assert_eq!(delay_from(Duration::from_secs(120), 3), Duration::from_secs(120));
    }
}
