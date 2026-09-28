//! Registry of running feeds, one per (camera, stream). A feed starts with
//! its first viewer and stops shortly after its last one leaves.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use sqlx::PgPool;
use tokio::sync::{Notify, broadcast, watch};

use super::facts::FactsCache;
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
    /// Last known facts per stream, kept after a feed closes.
    pub(super) facts: FactsCache,
    /// Cameras with only one stream: whoever asks for their substream
    /// joins their main feed (one RTSP session per camera, not two).
    single: Mutex<HashSet<String>>,
}

impl MediaHub {
    pub fn new(db: PgPool, credentials: Arc<CredentialStore>) -> Self {
        Self { db, credentials, feeds: Mutex::new(HashMap::new()), facts: FactsCache::default(), single: Mutex::default() }
    }

    /// Whether a camera has only a main stream (set when cameras load or change).
    pub fn set_single_stream(&self, camera_id: &str, single: bool) {
        let mut all = self.single.lock().expect("media hub lock");
        if single {
            all.insert(camera_id.to_string());
        } else {
            all.remove(camera_id);
        }
    }

    /// The feed that serves `kind` for a camera: a camera without a
    /// substream serves everything from its main feed.
    fn key(&self, camera_id: &str, kind: StreamKind) -> Key {
        let single = kind == StreamKind::Sub && self.single.lock().expect("media hub lock").contains(camera_id);
        (camera_id.to_string(), if single { StreamKind::Main } else { kind })
    }

    /// Join the feed for a camera stream, starting it if needed.
    pub fn subscribe(self: &Arc<Self>, camera_id: &str, kind: StreamKind) -> Subscription {
        let key = self.key(camera_id, kind);
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
        if let Some(ch) = self.feeds.lock().expect("media hub lock").get(&self.key(camera_id, kind)) {
            ch.preroll.set_keep(keep_secs);
        }
        sub
    }

    /// The last `secs` of frames (from a keyframe), if pre-record is kept.
    pub fn preroll(&self, camera_id: &str, kind: StreamKind, secs: u32) -> Vec<Frame> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&self.key(camera_id, kind)).map(|ch| ch.preroll.snapshot(secs)).unwrap_or_default()
    }

    /// Pre-record audio from `from_pts` on (the clip's first video frame).
    pub fn preroll_audio(&self, camera_id: &str, kind: StreamKind, from_pts: i64) -> Vec<AudioFrame> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&self.key(camera_id, kind)).map(|ch| ch.preroll.audio_since(from_pts)).unwrap_or_default()
    }

    /// A running feed's state; `None` when nobody uses that stream.
    pub fn feed_state(&self, camera_id: &str, kind: StreamKind) -> Option<FeedState> {
        let feeds = self.feeds.lock().expect("media hub lock");
        feeds.get(&self.key(camera_id, kind)).map(|ch| ch.state.borrow().clone())
    }

    /// Put a stream's state and facts on a camera's `stream`: `Active` with
    /// its measured rate while a feed delivers it, `Error` when that feed
    /// fails, else `Idle` with what it was last seen to be (the main stream
    /// is only opened while something uses it). `kinds`: the feeds that
    /// carry this stream, first choice first (a camera without a substream
    /// serves its main stream on the substream feed too).
    pub fn overlay_stream(&self, camera_id: &str, kinds: &[StreamKind], stream: &mut watchgrid_model::Stream) {
        let key = |k: StreamKind| self.key(camera_id, k);
        let open = kinds.iter().find_map(|k| self.feed_state(camera_id, *k).map(|s| (*k, s)));
        let known = open.as_ref().and_then(|(k, _)| self.facts.get(&key(*k))).or_else(|| kinds.iter().find_map(|k| self.facts.get(&key(*k))));
        stream.status = match &open {
            Some((_, FeedState::Streaming(_))) => watchgrid_model::StreamStatus::Active,
            Some((_, FeedState::Failed(_))) => watchgrid_model::StreamStatus::Error,
            _ => watchgrid_model::StreamStatus::Idle,
        };
        if let Some(f) = known {
            stream.codec = Some(f.info.codec_label());
            stream.width = Some(f.info.track.width);
            stream.height = Some(f.info.track.height);
            stream.audio_codec = f.info.audio_codec.clone();
            stream.fps = f.fps;
            stream.bitrate = f.kbps;
        }
    }

    /// The camera was changed or removed: its streams may be different now.
    pub fn forget_facts(&self, camera_id: &str) {
        self.facts.forget(camera_id);
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

    fn info(width: u32) -> Arc<TrackInfo> {
        Arc::new(TrackInfo { codec: "avc1.4d001e".into(), track: VideoTrack { codec: crate::media::VideoCodec::H264, width, height: 432, decoder_config: vec![] }, audio_codec: None, audio: None })
    }

    #[tokio::test]
    async fn a_stream_shows_what_its_feed_is_doing() {
        let key = ("cam".to_string(), StreamKind::Sub);
        let hub = hub_with_sub(FeedState::Streaming(info(768)));
        hub.facts.info(&key, info(768));
        hub.facts.rate(&key, 15.0, 480);
        let mut sub = Stream::unprobed("rtsp://h/2");
        hub.overlay_stream("cam", &[StreamKind::Sub], &mut sub);
        assert_eq!((sub.status, sub.codec.as_deref(), sub.width, sub.fps, sub.bitrate), (StreamStatus::Active, Some("H264"), Some(768), Some(15.0), Some(480)));

        let mut sub = Stream::unprobed("rtsp://h/2");
        hub_with_sub(FeedState::Failed("no".into())).overlay_stream("cam", &[StreamKind::Sub], &mut sub);
        assert_eq!(sub.status, StreamStatus::Error);
    }

    #[tokio::test]
    async fn a_closed_main_stream_is_idle_with_what_it_was() {
        let hub = hub_with_sub(FeedState::Connecting);
        hub.feeds.lock().unwrap().clear();
        let main = ("cam".to_string(), StreamKind::Main);
        hub.facts.info(&main, info(2560));
        hub.facts.rate(&main, 25.0, 4000);
        let mut stream = Stream::unprobed("rtsp://h/1");
        hub.overlay_stream("cam", &[StreamKind::Main], &mut stream);
        assert_eq!((stream.status, stream.width, stream.bitrate), (StreamStatus::Idle, Some(2560), Some(4000)), "opened on demand; the capacity estimate still has its bitrate");

        hub.forget_facts("cam");
        let mut stream = Stream::unprobed("rtsp://h/1");
        hub.overlay_stream("cam", &[StreamKind::Main], &mut stream);
        assert_eq!(stream.width, None, "other settings: nothing known yet");
    }

    #[tokio::test]
    async fn without_a_substream_the_main_stream_is_the_watched_feed() {
        let hub = hub_with_sub(FeedState::Streaming(info(1920)));
        hub.facts.info(&("cam".to_string(), StreamKind::Sub), info(1920));
        let mut main = Stream::unprobed("rtsp://h/1");
        hub.overlay_stream("cam", &[StreamKind::Main, StreamKind::Sub], &mut main);
        assert_eq!((main.status, main.width), (StreamStatus::Active, Some(1920)));
    }

    #[tokio::test]
    async fn a_camera_with_one_stream_uses_one_feed() {
        let db = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused").unwrap();
        let hub = Arc::new(MediaHub::new(db, Arc::new(CredentialStore::from_key(&[1u8; 32]))));
        hub.set_single_stream("single", true);
        let _health = hub.subscribe("single", StreamKind::Sub);
        let _live = hub.subscribe("single", StreamKind::Main);
        let _a = hub.subscribe("both", StreamKind::Sub);
        let _b = hub.subscribe("both", StreamKind::Main);
        let keys: Vec<Key> = hub.feeds.lock().unwrap().keys().cloned().collect();
        assert!(keys.contains(&("single".into(), StreamKind::Main)) && !keys.contains(&("single".into(), StreamKind::Sub)), "one session: {keys:?}");
        assert_eq!(keys.iter().filter(|(id, _)| id == "both").count(), 2, "with a substream: two feeds");
        hub.set_single_stream("single", false);
        assert_eq!(hub.key("single", StreamKind::Sub), ("single".into(), StreamKind::Sub), "a substream was added");
    }
}
