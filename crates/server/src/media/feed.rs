//! One feed's task: open the camera stream, publish frames to viewers,
//! reconnect on failure, and exit once nobody has watched for a while.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use retina::codec::{CodecItem, ParametersRef};

use super::hub::{Channels, Key, MediaHub};
use super::{FeedState, Frame, StreamKind, TrackInfo, boxes};
use crate::cameras;
use crate::rtsp::session::{self, Opened};
use crate::supervisor::backoff;

/// How long a feed stays open without viewers (covers page navigation).
const LINGER: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(1);

pub async fn run(hub: Arc<MediaHub>, key: Key, ch: Arc<Channels>) {
    let (id, kind) = (&key.0, key.1);
    let mut attempt = 0u32;
    let mut idle = Idle::default();
    loop {
        let reason = match open(&hub, id, kind).await {
            Err(e) => e,
            Ok(opened) => {
                attempt = 0;
                tracing::info!(camera = %id, ?kind, "live feed started");
                match pump(&hub, &key, &ch, opened, &mut idle).await {
                    Some(reason) => reason,
                    None => break, // no viewers left
                }
            }
        };
        attempt += 1;
        tracing::debug!(camera = %id, ?kind, "live feed failed: {reason}");
        ch.state.send_replace(FeedState::Failed(reason));

        // Back off, but keep checking whether anyone still watches.
        let until = Instant::now() + backoff::delay(attempt);
        while Instant::now() < until {
            tokio::time::sleep(TICK).await;
            if idle.expired(&hub, &key, &ch) {
                tracing::info!(camera = %id, ?kind, "live feed stopped");
                return;
            }
        }
        ch.state.send_replace(FeedState::Connecting);
    }
    tracing::info!(camera = %id, ?kind, "live feed stopped");
}

async fn open(hub: &MediaHub, id: &str, kind: StreamKind) -> Result<Opened, String> {
    let info = cameras::connection_info(&hub.db, &hub.credentials, id).await?.ok_or("camera not found")?;
    if !info.enabled {
        return Err("camera is disabled".into());
    }
    // A camera without a substream serves its main stream everywhere.
    let url = match kind {
        StreamKind::Sub => info.sub_url.unwrap_or(info.main_url),
        StreamKind::Main => info.main_url,
    };
    session::open(&url, &info.username, info.password.as_deref()).await
}

/// Forward frames until the stream fails (`Some(reason)`) or the feed goes idle (`None`).
async fn pump(hub: &MediaHub, key: &Key, ch: &Channels, mut opened: Opened, idle: &mut Idle) -> Option<String> {
    let mut ticker = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            item = opened.stream.next() => match item {
                None => return Some("the camera closed the stream".into()),
                Some(Err(e)) => return Some(format!("stream error: {e}")),
                Some(Ok(CodecItem::VideoFrame(f))) if f.stream_id() == opened.video => {
                    if f.has_new_parameters() || !matches!(*ch.state.borrow(), FeedState::Streaming(_)) {
                        match track_info(&opened) {
                            Ok(Some(info)) => { ch.state.send_replace(FeedState::Streaming(Arc::new(info))); }
                            Ok(None) => continue, // parameters not known yet
                            Err(e) => return Some(e),
                        }
                    }
                    let ts = f.timestamp();
                    let pts = rescale(ts.elapsed(), ts.clock_rate().get());
                    let keyframe = f.is_random_access_point();
                    // No receivers is fine; the idle check handles it.
                    let _ = ch.frames.send(Frame { pts, keyframe, data: f.into_data().into() });
                }
                Some(Ok(_)) => {}
            },
            _ = ch.reload.notified() => return Some("camera settings changed".into()),
            _ = ticker.tick() => if idle.expired(hub, key, ch) {
                return None;
            }
        }
    }
}

fn track_info(opened: &Opened) -> Result<Option<TrackInfo>, String> {
    let Some(ParametersRef::Video(v)) = opened.stream.streams()[opened.video].parameters() else { return Ok(None) };
    let codec = v.rfc6381_codec().to_string();
    if !codec.starts_with("avc1") {
        return Err(format!("live view supports H.264 only (camera sends {codec})"));
    }
    let (width, height) = v.pixel_dimensions();
    Ok(Some(TrackInfo { codec, track: super::VideoTrack { width, height, avcc: v.extra_data().to_vec() } }))
}

/// Convert RTP clock ticks to the 90 kHz MP4 timescale.
fn rescale(ticks: i64, clock_rate: u32) -> i64 {
    if clock_rate == boxes::TIMESCALE {
        ticks
    } else {
        (i128::from(ticks) * i128::from(boxes::TIMESCALE) / i128::from(clock_rate)) as i64
    }
}

/// Tracks how long a feed has had no viewers.
#[derive(Default)]
struct Idle(Option<Instant>);

impl Idle {
    /// True once the feed has been unwatched for `LINGER` and was released.
    fn expired(&mut self, hub: &MediaHub, key: &Key, ch: &Channels) -> bool {
        if ch.frames.receiver_count() > 0 {
            self.0 = None;
            return false;
        }
        let since = *self.0.get_or_insert_with(Instant::now);
        since.elapsed() >= LINGER && hub.release_if_idle(key)
    }
}

#[cfg(test)]
mod tests {
    use super::rescale;

    #[test]
    fn rescales_other_clock_rates() {
        assert_eq!(rescale(90_000, 90_000), 90_000);
        assert_eq!(rescale(48_000, 48_000 * 2), 45_000);
    }
}
