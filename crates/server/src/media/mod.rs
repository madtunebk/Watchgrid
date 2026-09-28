//! Live video for the browser: on-demand camera sessions shared by all
//! viewers, repackaged as fragmented MP4 (no transcoding) and streamed over
//! a WebSocket into Media Source Extensions.

pub mod audio;
pub(crate) mod boxes;
pub mod dump;
mod facts;
mod feed;
mod fmp4;
mod fragmenter;
mod hub;
mod nal;
mod preroll;
pub mod mp4;
pub mod mp4_read;
mod timing;
mod ws;

#[cfg(test)]
mod hevc_tests;

pub use boxes::{TIMESCALE, VideoCodec, VideoTrack};
pub use hub::{MediaHub, Subscription};
pub use timing::SampleClock;
pub use ws::upgrade;

use axum::body::Bytes;

/// Which of a camera's streams to watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamKind {
    Main,
    Sub,
}

/// Codec setup of a feed; changes when the camera sends new parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackInfo {
    /// RFC 6381 codec string for MSE, e.g. `avc1.640028`.
    pub codec: String,
    pub track: VideoTrack,
    /// Audio encoding announced by the camera.
    pub audio_codec: Option<String>,
    /// The Opus track carried alongside the video, if any.
    pub audio: Option<audio::AudioTrack>,
}

impl TrackInfo {
    /// Software motion detection requires H.264.
    pub fn is_h264(&self) -> bool {
        self.track.codec == VideoCodec::H264
    }

    pub fn can_mux(&self) -> bool {
        matches!(self.track.codec, VideoCodec::H264 | VideoCodec::H265)
    }

    /// For people: "H264", "HEVC"…
    pub fn codec_label(&self) -> String {
        match self.track.codec {
            VideoCodec::H264 => "H264".into(),
            VideoCodec::H265 => "HEVC".into(),
            VideoCodec::Unsupported => self.codec.split('.').next().unwrap_or(&self.codec).to_uppercase(),
        }
    }
}

/// One encoded video frame (H.264 or HEVC, length-prefixed NALs).
#[derive(Debug, Clone)]
pub struct Frame {
    /// Presentation time in 90 kHz ticks, from the camera's RTP clock.
    pub pts: i64,
    pub keyframe: bool,
    pub data: Bytes,
}

/// What viewers see of a feed.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedState {
    Connecting,
    Streaming(std::sync::Arc<TrackInfo>),
    Failed(String),
}
