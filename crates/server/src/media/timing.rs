//! Sample durations from camera timestamps. A frame's duration is the gap
//! to the next frame; broken or missing timestamps fall back to the last
//! good duration so the timeline stays continuous.

use super::boxes::TIMESCALE;

/// Used when timestamps don't advance (e.g. duplicated RTP timestamps).
const FALLBACK: u32 = TIMESCALE / 25;
/// A gap longer than this is treated as a discontinuity, not a long frame.
const MAX: i64 = TIMESCALE as i64 * 2;

#[derive(Default)]
pub struct SampleClock {
    last: Option<u32>,
}

impl SampleClock {
    /// Duration of the frame at `pts` given the next frame's `next_pts`.
    pub fn duration(&mut self, pts: i64, next_pts: i64) -> u32 {
        let delta = next_pts - pts;
        let d = if delta > 0 && delta <= MAX { delta as u32 } else { self.last.unwrap_or(FALLBACK) };
        self.last = Some(d);
        d
    }

    /// Duration for a final frame with no successor.
    pub fn last(&self) -> u32 {
        self.last.unwrap_or(FALLBACK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_the_last_good_duration() {
        let mut c = SampleClock::default();
        assert_eq!(c.duration(0, 3000), 3000);
        assert_eq!(c.duration(3000, 3000), 3000, "no advance");
        assert_eq!(c.duration(3000, 10_000_000), 3000, "huge jump");
        assert_eq!(c.last(), 3000);
        assert_eq!(SampleClock::default().duration(5, 1), FALLBACK);
    }
}
