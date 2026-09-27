//! Registry of running feeds, one per (camera, stream). A feed starts with
//! its first viewer and stops shortly after its last one leaves.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sqlx::PgPool;
use tokio::sync::{Notify, broadcast, watch};

use super::preroll::Preroll;
use super::audio::AudioFrame;
use super::{FeedState, Frame, StreamKind, feed};
use crate::credentials::CredentialStore;

/// Frames buffered per viewer before it is considered lagging (~10 s at 25 fps).
const FRAME_BUFFER: usize = 256;
/// Audio packets buffered per viewer (~10 s of 20 ms packets).
const AUDIO_BUFFER: usize = 512;

pub type Key = (String, StreamKind);

/// Sending halves of a feed, owned by the hub.
pub struct Channels {
    pub frames: broadcast::Sender<Frame>,
    /// Opus packets, when the camera has audio.
    pub audio: broadcast::Sender<AudioFrame>,
    pub state: watch::Sender<FeedState>,
    /// Camera settings changed: drop the session and reconnect.
    pub reload: Notify,
    /// Recent frames for pre-record (empty unless requested).
    pub preroll: Preroll,
}

/// A viewer's handle on a feed.
pub struct Subscription {
    pub frames: broadcast::Receiver<Frame>,
    /// Unread audio costs nothing: old packets are simply overwritten.
    pub audio: broadcast::Receiver<AudioFrame>,
    pub state: watch::Receiver<FeedState>,
}

pub struct MediaHub {
    pub(super) db: PgPool,
    pub(super) credentials: Arc<CredentialStore>,
    pub(super) feeds: Mutex<HashMap<Key, Arc<Channels>>>,
}

impl MediaHub {
    pub fn new(db: PgPool, credentials: Arc<CredentialStore>) -> Self {
        Self { db, credentials, feeds: Mutex::new(HashMap::new()) }
    }

    /// Join the feed for a camera stream, starting it if needed.
    pub fn subscribe(self: &Arc<Self>, camera_id: &str, kind: StreamKind) -> Subscription {
        let key = (camera_id.to_string(), kind);
        let mut feeds = self.feeds.lock().expect("media hub lock");
        let channels = feeds.entry(key.clone()).or_insert_with(|| {
            let channels = Arc::new(Channels {
                frames: broadcast::channel(FRAME_BUFFER).0,
                audio: broadcast::channel(AUDIO_BUFFER).0,
                state: watch::channel(FeedState::Connecting).0,
                reload: Notify::new(),
                preroll: Preroll::default(),
            });
            tokio::spawn(feed::run(self.clone(), key, channels.clone()));
            channels
        });
        Subscription { frames: channels.frames.subscribe(), audio: channels.audio.subscribe(), state: channels.state.subscribe() }
    }

    /// Join a feed and keep `keep_secs` of recent frames for pre-record.
    pub fn subscribe_with_preroll(self: &Arc<Self>, camera_id: &str, kind: StreamKind, keep_secs: u32) -> Subscription {
        let sub = self.subscribe(camera_id, kind);
        if let Some(ch) = self.feeds.lock().expect("media hub lock").get(&(camera_id.to_string(), kind)) {
            ch.preroll.set_keep(keep_secs);
        }
        sub
    }

    /// The last `secs` of frames (from a keyframe), if pre-record is kept.
    pub fn preroll(&self, camera_id: &str, kind: StreamKind, secs: u32) -> Vec<Frame> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&(camera_id.to_string(), kind)).map(|ch| ch.preroll.snapshot(secs)).unwrap_or_default()
    }

    /// Pre-record audio from `from_pts` on (the clip's first video frame).
    pub fn preroll_audio(&self, camera_id: &str, kind: StreamKind, from_pts: i64) -> Vec<AudioFrame> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&(camera_id.to_string(), kind)).map(|ch| ch.preroll.audio_since(from_pts)).unwrap_or_default()
    }

    /// A running feed's state; `None` when nobody uses that stream.
    pub fn feed_state(&self, camera_id: &str, kind: StreamKind) -> Option<FeedState> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&(camera_id.to_string(), kind)).map(|ch| ch.state.borrow().clone())
    }

    /// Put the substream's state on a camera's `sub`: active while motion
    /// detection or a live preview uses it (the main stream's state comes
    /// from the camera's supervisor).
    pub fn overlay_sub(&self, camera_id: &str, sub: Option<&mut watchgrid_model::Stream>) {
        let Some(sub) = sub else { return };
        match self.feed_state(camera_id, StreamKind::Sub) {
            Some(FeedState::Streaming(info)) => {
                sub.status = watchgrid_model::StreamStatus::Active;
                sub.codec = Some(info.codec_label());
                sub.width = Some(info.track.width);
                sub.height = Some(info.track.height);
                sub.audio_codec = info.audio_codec.clone();
            }
            Some(FeedState::Failed(_)) => sub.status = watchgrid_model::StreamStatus::Error,
            Some(FeedState::Connecting) | None => {}
        }
    }

    /// Feeds currently open (live viewers and recordings).
    pub fn active_feeds(&self) -> usize {
        self.feeds.lock().expect("media hub lock").len()
    }

    /// Make a camera's running feeds reconnect with its current settings
    /// (or stop delivering video if it was disabled or deleted).
    pub fn reload(&self, camera_id: &str) {
        let feeds = self.feeds.lock().expect("media hub lock");
        for ((id, _), channels) in feeds.iter() {
            if id == camera_id {
                channels.reload.notify_one();
            }
        }
    }

    /// Remove a feed if nobody watches it. Checked under the same lock as
    /// `subscribe`, so a viewer can never join a feed that is shutting down.
    pub(super) fn release_if_idle(&self, key: &Key) -> bool {
        let mut feeds = self.feeds.lock().expect("media hub lock");
        let idle = feeds.get(key).is_none_or(|c| c.frames.receiver_count() == 0);
        if idle {
            feeds.remove(key);
        }
        idle
    }
}

#[cfg(test)]
mod tests {
    use watchgrid_model::{Stream, StreamStatus};

    use super::*;
    use crate::media::{TrackInfo, VideoTrack};

    fn hub_with_sub(state: FeedState) -> MediaHub {
        let db = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap();
        let hub = MediaHub::new(db, Arc::new(CredentialStore::from_key(&[1u8; 32])));
        let channels = Arc::new(Channels {
            frames: broadcast::channel(1).0,
            audio: broadcast::channel(1).0,
            state: watch::channel(state).0,
            reload: Notify::new(),
            preroll: Preroll::default(),
        });
        hub.feeds.lock().unwrap().insert(("cam".into(), StreamKind::Sub), channels);
        hub
    }

    #[tokio::test]
    async fn the_substream_shows_what_its_feed_is_doing() {
        let info = TrackInfo { codec: "avc1.4d001e".into(), track: VideoTrack { codec: crate::media::VideoCodec::H264, width: 768, height: 432, decoder_config: vec![] }, audio_codec: None, audio: None };
        let mut sub = Stream::unprobed("rtsp://h/2");
        hub_with_sub(FeedState::Streaming(Arc::new(info))).overlay_sub("cam", Some(&mut sub));
        assert_eq!((sub.status, sub.codec.as_deref(), sub.width, sub.height), (StreamStatus::Active, Some("H264"), Some(768), Some(432)));

        let mut sub = Stream::unprobed("rtsp://h/2");
        hub_with_sub(FeedState::Failed("no".into())).overlay_sub("cam", Some(&mut sub));
        assert_eq!(sub.status, StreamStatus::Error);

        let mut sub = Stream::unprobed("rtsp://h/2");
        let hub = hub_with_sub(FeedState::Connecting);
        hub.feeds.lock().unwrap().clear();
        hub.overlay_sub("cam", Some(&mut sub));
        assert_eq!(sub.status, StreamStatus::Idle, "no feed: idle, opened on demand");
    }
}
