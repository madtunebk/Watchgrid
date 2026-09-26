//! Folding bursts of "this changed" notices: the first one refreshes at
//! once, more within `GAP_MS` become a single refresh at the end of it. A
//! camera pulsing motion then costs one refetch every few seconds, not one
//! per pulse, and nothing is ever left stale.

/// Minimum time between two refreshes of one topic.
pub const GAP_MS: f64 = 3000.0;

#[derive(Debug, Default, Clone, Copy)]
pub struct Gate {
    last: Option<f64>,
    pending: bool,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    /// Refresh now.
    Now,
    /// Refresh after this many ms (schedule it).
    Later(f64),
    /// A refresh is already scheduled.
    Folded,
}

impl Gate {
    /// A notice arrived at `now` (ms).
    pub fn notice(&mut self, now: f64) -> Action {
        if self.pending {
            return Action::Folded;
        }
        match self.last {
            Some(last) if now - last < GAP_MS => {
                self.pending = true;
                Action::Later(last + GAP_MS - now)
            }
            _ => {
                self.last = Some(now);
                Action::Now
            }
        }
    }

    /// The scheduled refresh ran at `now`.
    pub fn fired(&mut self, now: f64) {
        self.pending = false;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_becomes_two_refreshes_and_quiet_ones_pass_at_once() {
        let mut g = Gate::default();
        assert_eq!(g.notice(0.0), Action::Now);
        assert_eq!(g.notice(500.0), Action::Later(2500.0));
        assert_eq!(g.notice(900.0), Action::Folded);
        assert_eq!(g.notice(2900.0), Action::Folded);
        g.fired(3000.0);
        assert_eq!(g.notice(3100.0), Action::Later(2900.0), "still within the gap after the last refresh");
        g.fired(6000.0);
        assert_eq!(g.notice(20_000.0), Action::Now, "after a quiet spell: immediately");
    }
}
