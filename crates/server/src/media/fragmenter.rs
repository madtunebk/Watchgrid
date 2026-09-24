//! Per-viewer conversion of frames into fMP4 media segments.
//!
//! A frame's duration is only known when the next one arrives, so each
//! frame is emitted one frame late. Output starts at a keyframe and decode
//! time starts at zero for every viewer.

use super::timing::SampleClock;
use super::{Frame, fmp4};

#[derive(Default)]
pub struct Fragmenter {
    sequence: u32,
    /// Decode time already emitted, in timescale ticks.
    decode_time: u64,
    pending: Option<Frame>,
    clock: SampleClock,
}

impl Fragmenter {
    /// Feed the next frame; returns the segment for the previous one.
    pub fn push(&mut self, frame: Frame) -> Option<Vec<u8>> {
        let Some(prev) = self.pending.take() else {
            // Nothing can be decoded before the first keyframe.
            if frame.keyframe {
                self.pending = Some(frame);
            }
            return None;
        };
        let duration = self.clock.duration(prev.pts, frame.pts);
        self.pending = Some(frame);
        Some(self.emit(prev, duration))
    }

    /// Drop the buffered frame and wait for the next keyframe (after lost frames).
    pub fn resync(&mut self) {
        self.pending = None;
    }

    fn emit(&mut self, frame: Frame, duration: u32) -> Vec<u8> {
        self.sequence += 1;
        let out = fmp4::media_segment(self.sequence, self.decode_time, duration, frame.keyframe, &frame.data);
        self.decode_time += u64::from(duration);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts: i64, keyframe: bool) -> Frame {
        Frame { pts, keyframe, data: vec![0, 0, 0, 1, 0x65].into() }
    }

    #[test]
    fn waits_for_a_keyframe_then_lags_one_frame() {
        let mut f = Fragmenter::default();
        assert!(f.push(frame(0, false)).is_none(), "leading non-keyframe dropped");
        assert!(f.push(frame(3600, true)).is_none(), "keyframe buffered");
        assert!(f.push(frame(7200, false)).is_some());
        assert_eq!(f.decode_time, 3600);
        assert_eq!(f.sequence, 1);
    }

    #[test]
    fn bad_timestamps_reuse_the_last_duration() {
        let mut f = Fragmenter::default();
        f.push(frame(0, true));
        f.push(frame(3000, false));
        f.push(frame(3000, false)); // no advance
        assert_eq!(f.decode_time, 6000);
        f.push(frame(10_000_000, false)); // huge jump
        assert_eq!(f.decode_time, 9000);
    }

    #[test]
    fn resync_waits_for_the_next_keyframe() {
        let mut f = Fragmenter::default();
        f.push(frame(0, true));
        f.resync();
        assert!(f.push(frame(3000, false)).is_none());
        assert!(f.push(frame(6000, true)).is_none());
        assert!(f.push(frame(9000, false)).is_some());
    }
}
