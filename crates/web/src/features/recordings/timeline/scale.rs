//! Time ↔ position on the day axis. Pure math, no UI.

use chrono::{DateTime, Duration, Local, Utc};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl Scale {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    fn span_ms(&self) -> f64 {
        (self.end - self.start).num_milliseconds().max(1) as f64
    }

    /// Position of `t` in percent of the day, clamped to 0..=100.
    pub fn pct(&self, t: DateTime<Utc>) -> f64 {
        ((t - self.start).num_milliseconds() as f64 / self.span_ms() * 100.0).clamp(0.0, 100.0)
    }

    /// `(left %, width %)` of a span, clipped to the day. Very short
    /// spans keep a minimum width so they stay clickable.
    pub fn span(&self, from: DateTime<Utc>, to: DateTime<Utc>, zoom: u8) -> Option<(f64, f64)> {
        if to <= self.start || from >= self.end {
            return None;
        }
        let left = self.pct(from);
        let min = 0.12 / zoom as f64;
        Some((left, (self.pct(to) - left).max(min)))
    }

    /// The time at `fraction` (0..=1) of the day.
    pub fn at(&self, fraction: f64) -> DateTime<Utc> {
        self.start + Duration::milliseconds((self.span_ms() * fraction.clamp(0.0, 1.0)) as i64)
    }

    pub fn contains(&self, t: DateTime<Utc>) -> bool {
        t >= self.start && t < self.end
    }

    /// Tick marks `(percent, "HH:MM")`, denser when zoomed in.
    pub fn ticks(&self, zoom: u8) -> Vec<(f64, String)> {
        let step = match zoom {
            1 => 120,
            2 => 60,
            4 => 30,
            8 => 15,
            _ => 5,
        };
        let mut out = Vec::new();
        let mut t = self.start;
        while t < self.end {
            out.push((self.pct(t), crate::format::time_hm(t.with_timezone(&Local))));
            t += Duration::minutes(step);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_and_time_convert_both_ways() {
        let start: DateTime<Utc> = "2026-09-26T00:00:00Z".parse().unwrap();
        let s = Scale::new(start, start + Duration::hours(24));
        let noon = start + Duration::hours(12);
        assert_eq!(s.at(0.5), noon);
        assert!((s.pct(noon) - 50.0).abs() < 1e-9);
        assert_eq!(s.at(2.0), start + Duration::hours(24), "clamped");
    }
}
