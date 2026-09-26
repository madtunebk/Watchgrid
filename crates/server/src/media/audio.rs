//! Camera audio for browsers and recordings. Cameras send G.711 (A-law or
//! µ-law, 8 kHz), which neither Media Source Extensions nor browsers'
//! MP4 playback accept; it is re-encoded to Opus (pure Rust, 20 ms packets,
//! ~24 kbit/s). Nothing else about the audio is changed.

use axum::body::Bytes;
use opus_pure::{Application, MAX_PACKET_BYTES, OpusEncoder};

use super::boxes::TIMESCALE;

/// Input rate of G.711.
pub const INPUT_RATE: u32 = 8000;
/// Samples per Opus packet at the input rate (20 ms).
const FRAME: usize = INPUT_RATE as usize / 50;
/// Opus is always timed at 48 kHz in MP4.
pub const OPUS_TIMESCALE: u32 = 48_000;
/// One packet's duration at [`OPUS_TIMESCALE`].
pub const PACKET_DURATION: u32 = OPUS_TIMESCALE / 50;
/// One packet's duration in [`TIMESCALE`] (90 kHz) ticks.
const PACKET_TICKS: i64 = TIMESCALE as i64 / 50;
/// A jump this large in camera timestamps restarts the packet timeline.
const MAX_DRIFT: i64 = TIMESCALE as i64 / 10;
const BITRATE: i32 = 24_000;

/// Audio codecs Watchgrid can take from cameras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Alaw,
    Ulaw,
}

impl Source {
    /// From the RTSP encoding name (`PCMA`, `PCMU`).
    pub fn from_encoding(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "pcma" => Some(Self::Alaw),
            "pcmu" => Some(Self::Ulaw),
            _ => None,
        }
    }

    fn decode(self, b: u8) -> i16 {
        match self {
            Self::Alaw => alaw(b),
            Self::Ulaw => ulaw(b),
        }
    }
}

/// What a player needs to know about the audio track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioTrack {
    pub channels: u8,
    /// Decoder delay to drop, in 48 kHz samples.
    pub pre_skip: u16,
    /// The camera's original rate (informational).
    pub input_rate: u32,
}

/// One Opus packet. `pts` is in [`TIMESCALE`] ticks like video frames.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub pts: i64,
    pub data: Bytes,
}

pub struct Transcoder {
    source: Source,
    encoder: OpusEncoder,
    /// Samples not yet encoded, and the time of the first one.
    pending: Vec<f32>,
    pending_pts: i64,
    packet: Vec<u8>,
}

impl Transcoder {
    pub fn new(source: Source) -> Result<Self, String> {
        let mut encoder = OpusEncoder::new(INPUT_RATE as i32, 1, Application::Voip).map_err(|e| format!("Opus encoder: {e}"))?;
        encoder.bitrate_bps = BITRATE;
        Ok(Self { source, encoder, pending: Vec::with_capacity(FRAME * 4), pending_pts: 0, packet: vec![0; MAX_PACKET_BYTES] })
    }

    pub fn track(&self) -> AudioTrack {
        // Encoder delay at 8 kHz, expressed at 48 kHz.
        let pre_skip = (self.encoder.lookahead() * (OPUS_TIMESCALE / INPUT_RATE) as usize).min(usize::from(u16::MAX)) as u16;
        AudioTrack { channels: 1, pre_skip, input_rate: INPUT_RATE }
    }

    /// Feed one camera packet (G.711 bytes) taken at `pts`; returns the
    /// Opus packets now complete.
    pub fn push(&mut self, pts: i64, data: &[u8]) -> Vec<AudioFrame> {
        let expected = self.pending_pts + self.pending.len() as i64 * TIMESCALE as i64 / i64::from(INPUT_RATE);
        if self.pending.is_empty() || (pts - expected).abs() > MAX_DRIFT {
            // First packet, or a gap/jump: start a new run at the camera's time.
            self.pending.clear();
            self.pending_pts = pts;
        }
        self.pending.extend(data.iter().map(|b| f32::from(self.source.decode(*b)) / 32768.0));
        let mut out = Vec::new();
        while self.pending.len() >= FRAME {
            match self.encoder.encode(&self.pending[..FRAME], FRAME, &mut self.packet) {
                Ok(n) => out.push(AudioFrame { pts: self.pending_pts, data: Bytes::copy_from_slice(&self.packet[..n]) }),
                Err(e) => tracing::debug!("Opus encode failed: {e}"),
            }
            self.pending.drain(..FRAME);
            self.pending_pts += PACKET_TICKS;
        }
        out
    }
}

/// ITU-T G.711 A-law to linear PCM.
fn alaw(b: u8) -> i16 {
    let a = b ^ 0x55;
    let seg = (a & 0x70) >> 4;
    let mut t = i16::from(a & 0x0f) << 4;
    t = match seg {
        0 => t + 8,
        1 => t + 0x108,
        _ => (t + 0x108) << (seg - 1),
    };
    if a & 0x80 != 0 { t } else { -t }
}

/// ITU-T G.711 µ-law to linear PCM.
fn ulaw(b: u8) -> i16 {
    let u = !b;
    let t = ((i16::from(u & 0x0f) << 3) + 0x84) << ((u & 0x70) >> 4);
    if u & 0x80 != 0 { 0x84 - t } else { t - 0x84 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g711_decodes_to_the_standard_values() {
        // Reference points from the ITU-T tables.
        assert_eq!(alaw(0xd5), 8);
        assert_eq!(alaw(0x55), -8);
        assert_eq!(alaw(0xaa), 32256);
        assert_eq!(alaw(0x2a), -32256);
        assert_eq!(ulaw(0xff), 0);
        assert_eq!(ulaw(0x80), 32124);
        assert_eq!(ulaw(0x00), -32124);
    }

    #[test]
    fn packets_are_20ms_and_timed_from_the_camera() {
        let mut t = Transcoder::new(Source::Alaw).unwrap();
        // 40 ms camera packets (320 samples) of silence (A-law 0xd5).
        let silence = [0xd5u8; 320];
        let a = t.push(9_000, &silence);
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].pts, a[1].pts), (9_000, 9_000 + 1_800));
        let b = t.push(9_000 + 3_600, &silence);
        assert_eq!(b[0].pts, 9_000 + 3_600, "continues without a gap");
        // A jump restarts the timeline at the camera's time.
        let c = t.push(90_000, &silence);
        assert_eq!(c[0].pts, 90_000);
        assert!(t.track().pre_skip > 0);
        assert_eq!(Source::from_encoding("PCMA"), Some(Source::Alaw));
        assert_eq!(Source::from_encoding("MPEG4-GENERIC"), None);
    }
}
