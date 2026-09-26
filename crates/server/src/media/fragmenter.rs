//! Per-viewer conversion of frames into fMP4 media segments.
//!
//! A frame's duration is only known when the next one arrives, so each
//! frame is emitted one frame late. Output starts at a keyframe and decode
//! time starts at zero for every viewer. Audio packets (fixed 20 ms) are
//! placed on the same timeline through the last video frame sent.

use super::audio::{AudioFrame, OPUS_TIMESCALE, PACKET_DURATION};
use super::boxes::TIMESCALE;
use super::timing::SampleClock;
use super::{Frame, boxes, fmp4};

#[derive(Default)]
pub struct Fragmenter {
    sequence: u32,
    /// Decode time already emitted, in timescale ticks.
    decode_time: u64,
    pending: Option<Frame>,
    clock: SampleClock,
    /// Camera time and decode time of the last video frame sent: where
    /// audio goes. `None` until video flows (and after a resync).
    anchor: Option<(i64, u64)>,
    /// End of the last audio packet sent, in 48 kHz ticks.
    audio_end: u64,
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
        self.anchor = None;
    }

    /// Place an audio packet next to the video; `None` before video flows.
    pub fn push_audio(&mut self, a: &AudioFrame) -> Option<Vec<u8>> {
        let (video_pts, video_time) = self.anchor?;
        let at = video_time as i64 + (a.pts - video_pts);
        if at < 0 {
            return None;
        }
        // Never overlap the previous packet (camera timestamps jitter).
        let at = (at as u64 * u64::from(OPUS_TIMESCALE) / u64::from(TIMESCALE)).max(self.audio_end);
        self.sequence += 1;
        self.audio_end = at + u64::from(PACKET_DURATION);
        Some(fmp4::media_segment(self.sequence, boxes::AUDIO_TRACK, at, PACKET_DURATION, true, &a.data))
    }

    fn emit(&mut self, frame: Frame, duration: u32) -> Vec<u8> {
        self.sequence += 1;
        let out = fmp4::media_segment(self.sequence, boxes::VIDEO_TRACK, self.decode_time, duration, frame.keyframe, &frame.data);
        self.anchor = Some((frame.pts, self.decode_time));
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

    #[test]
    fn audio_follows_the_video_timeline() {
        let audio = |pts: i64| AudioFrame { pts, data: vec![0xfc].into() };
        let mut f = Fragmenter::default();
        assert!(f.push_audio(&audio(0)).is_none(), "no video yet");
        f.push(frame(90_000, true));
        f.push(frame(93_600, false)); // video frame at camera 90 000 sent at decode time 0
        let seg = f.push_audio(&audio(90_000 + 1_800)).expect("placed");
        let tfdt = crate::media::boxes::test_util::find(&seg, b"tfdt");
        let at = u64::from_be_bytes(seg[tfdt + 12..tfdt + 20].try_into().unwrap());
        assert_eq!(at, 960, "20 ms after the video frame, at 48 kHz");
        assert!(f.push_audio(&audio(0)).is_none(), "before the stream start");
        f.resync();
        assert!(f.push_audio(&audio(95_000)).is_none(), "waits for video after a resync");
    }
}
