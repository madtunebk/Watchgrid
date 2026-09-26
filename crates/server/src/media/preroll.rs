//! Recent frames of a feed, kept only while someone asked for pre-record,
//! so an event recording can start a few seconds before the event. Audio
//! is kept alongside for the same time.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use super::Frame;
use super::audio::AudioFrame;
use super::boxes::TIMESCALE;

/// Video pre-record starts at a keyframe up to one interval earlier than
/// asked; audio keeps this much more so it covers that.
const AUDIO_SLACK_SECS: u32 = 10;

#[derive(Default)]
pub struct Preroll {
    /// Seconds to keep; 0 = keep nothing.
    keep_secs: AtomicU32,
    frames: Mutex<VecDeque<Frame>>,
    audio: Mutex<VecDeque<AudioFrame>>,
}

impl Preroll {
    pub fn set_keep(&self, secs: u32) {
        self.keep_secs.store(secs, Ordering::Relaxed);
        if secs == 0 {
            self.clear();
        }
    }

    pub fn clear(&self) {
        self.frames.lock().expect("preroll lock").clear();
        self.audio.lock().expect("preroll lock").clear();
    }

    /// Keep audio for as long as video (plus slack for the keyframe start).
    pub fn push_audio(&self, packet: &AudioFrame) {
        let keep = self.keep_secs.load(Ordering::Relaxed);
        if keep == 0 {
            return;
        }
        let mut audio = self.audio.lock().expect("preroll lock");
        audio.push_back(packet.clone());
        let cutoff = packet.pts - i64::from(keep + AUDIO_SLACK_SECS) * i64::from(TIMESCALE);
        while audio.front().is_some_and(|p| p.pts < cutoff) {
            audio.pop_front();
        }
    }

    /// Audio packets from `from_pts` on (the pre-record's first video frame).
    pub fn audio_since(&self, from_pts: i64) -> Vec<AudioFrame> {
        self.audio.lock().expect("preroll lock").iter().filter(|p| p.pts >= from_pts).cloned().collect()
    }

    /// Add a frame, dropping what is older than needed. Keeps one extra
    /// keyframe interval so a clip can always start on a keyframe.
    pub fn push(&self, frame: &Frame) {
        let keep = self.keep_secs.load(Ordering::Relaxed);
        if keep == 0 {
            return;
        }
        let mut frames = self.frames.lock().expect("preroll lock");
        frames.push_back(frame.clone());
        let cutoff = frame.pts - i64::from(keep) * i64::from(TIMESCALE);
        // Drop from the front while the *next* keyframe is still old enough.
        while let Some(next_key) = frames.iter().skip(1).position(|f| f.keyframe).map(|i| i + 1) {
            if frames[next_key].pts <= cutoff {
                frames.drain(..next_key);
            } else {
                break;
            }
        }
    }

    /// Frames from the last keyframe at or before `secs` ago up to now.
    pub fn snapshot(&self, secs: u32) -> Vec<Frame> {
        let frames = self.frames.lock().expect("preroll lock");
        let Some(newest) = frames.back().map(|f| f.pts) else { return Vec::new() };
        let cutoff = newest - i64::from(secs) * i64::from(TIMESCALE);
        let start = frames.iter().rposition(|f| f.keyframe && f.pts <= cutoff).or_else(|| frames.iter().position(|f| f.keyframe));
        start.map(|i| frames.iter().skip(i).cloned().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1 fps with a keyframe every 4 s.
    fn frames(n: i64) -> Vec<Frame> {
        (0..n).map(|s| Frame { pts: s * i64::from(TIMESCALE), keyframe: s % 4 == 0, data: vec![0u8; 4].into() }).collect()
    }

    #[test]
    fn keeps_enough_to_start_on_a_keyframe() {
        let p = Preroll::default();
        for f in frames(20) {
            p.push(&f); // nothing kept yet
        }
        assert!(p.snapshot(5).is_empty());
        p.set_keep(5);
        for f in frames(20) {
            p.push(&f);
        }
        // Newest is 19 s; 5 s back is 14 s; the keyframe at or before it is 12 s.
        let snap = p.snapshot(5);
        assert_eq!(snap.first().map(|f| f.pts / i64::from(TIMESCALE)), Some(12));
        assert!(snap[0].keyframe);
        assert_eq!(snap.last().map(|f| f.pts / i64::from(TIMESCALE)), Some(19));
        p.set_keep(0);
        assert!(p.snapshot(5).is_empty());
    }

    #[test]
    fn audio_is_kept_as_long_as_video_and_cut_at_the_clip_start() {
        let p = Preroll::default();
        p.set_keep(5);
        for s in 0..30 {
            p.push_audio(&AudioFrame { pts: s * i64::from(TIMESCALE), data: vec![1].into() });
        }
        // Newest 29 s; 5 + 10 s kept.
        assert_eq!(p.audio_since(0).first().map(|a| a.pts / i64::from(TIMESCALE)), Some(14));
        assert_eq!(p.audio_since(20 * i64::from(TIMESCALE)).len(), 10);
        p.set_keep(0);
        assert!(p.audio_since(0).is_empty());
    }
}
