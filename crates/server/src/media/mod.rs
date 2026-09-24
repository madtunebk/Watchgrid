//! Live video for the browser: on-demand camera sessions shared by all
//! viewers, repackaged as fragmented MP4 (no transcoding) and streamed over
//! a WebSocket into Media Source Extensions.

mod boxes;
mod feed;
mod fmp4;
mod fragmenter;
mod hub;
mod preroll;
pub mod mp4;
mod timing;
mod ws;

pub use boxes::{TIMESCALE, VideoTrack};
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
    /// Audio encoding announced by the camera (not carried yet).
    pub audio_codec: Option<String>,
}

impl TrackInfo {
    /// Only H.264 can be repackaged for browsers and MP4 files today.
    pub fn is_h264(&self) -> bool {
        self.codec.starts_with("avc1")
    }
}

/// One encoded video frame (an H.264 access unit, length-prefixed NALs).
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
