//! One feed's task: open the camera stream, publish frames to viewers,
//! reconnect on failure, and exit once nobody has watched for a while.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use retina::codec::{CodecItem, ParametersRef};

use super::audio::Transcoder;
use super::hub::{Channels, Key, MediaHub};
use super::{FeedState, Frame, StreamKind, TrackInfo, boxes};
use crate::cameras;
use crate::rtsp::session::{self, Opened};
use crate::supervisor::backoff;

/// How long a feed stays open without viewers (covers page navigation).
const LINGER: Duration = Duration::from_secs(5);
/// A session with audio that fails this soon (some Tapo firmware drops the
/// connection at PLAY once audio is set up) is retried without audio.
const AUDIO_TRIAL: Duration = Duration::from_secs(15);
const TICK: Duration = Duration::from_secs(1);
/// Frame rate and bitrate are measured over this long.
const RATE_WINDOW: Duration = Duration::from_secs(2);

pub async fn run(hub: Arc<MediaHub>, key: Key, ch: Arc<Channels>) {
    let (id, kind) = (&key.0, key.1);
    let mut attempt = 0u32;
    let mut idle = Idle::default();
    // Audio only on the main stream: Tapo cameras refuse PLAY on a second
    // session with audio, so one per camera. And it's a bonus: dropped for
    // this feed if it makes the camera fail.
    let mut audio = kind == StreamKind::Main;
    loop {
        let started = Instant::now();
        let reason = match open(&hub, id, kind, audio).await {
            Err(e) => e,
            Ok(opened) => {
                attempt = 0;
                tracing::info!(camera = %id, ?kind, "live feed started");
                match pump(&hub, &key, &ch, opened, &mut idle).await {
                    PumpEnd::Failed(reason) => reason,
                    PumpEnd::Idle => break, // no viewers left
                    // New settings: reconnect right away; not an outage.
                    PumpEnd::Reload => {
                        tracing::info!(camera = %id, ?kind, "reconnecting with new settings");
                        ch.state.send_replace(FeedState::Connecting);
                        attempt = 0;
                        continue;
                    }
                }
            }
        };
        if audio && started.elapsed() < AUDIO_TRIAL {
            audio = false;
            tracing::warn!(camera = %id, ?kind, "stream failed with audio ({reason}); continuing without audio");
            ch.state.send_replace(FeedState::Connecting);
            continue;
        }
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

async fn open(hub: &MediaHub, id: &str, kind: StreamKind, audio: bool) -> Result<Opened, String> {
    let info = cameras::connection_info(&hub.db, &hub.credentials, id).await?.ok_or("camera not found")?;
    if !info.enabled {
        return Err("camera is disabled".into());
    }
    // A camera without a substream serves its main stream everywhere.
    let url = match kind {
        StreamKind::Sub => info.sub_url.unwrap_or(info.main_url),
        StreamKind::Main => info.main_url,
    };
    session::open(&url, &info.username, info.password.as_deref(), audio).await
}

/// Why forwarding stopped.
enum PumpEnd {
    Failed(String),
    /// Nobody watches any more.
    Idle,
    /// Camera settings changed.
    Reload,
}

/// Forward frames until the stream fails, the feed goes idle or settings change.
async fn pump(hub: &MediaHub, key: &Key, ch: &Channels, mut opened: Opened, idle: &mut Idle) -> PumpEnd {
    // Timestamps restart with every session.
    ch.preroll.clear();
    let mut audio = opened.audio.and_then(|(index, source, rate)| match Transcoder::new(source) {
        Ok(t) => Some((index, rate, t)),
        Err(e) => {
            tracing::warn!("camera audio unavailable: {e}");
            None
        }
    });
    let audio_track = audio.as_ref().map(|(_, _, t)| t.track());
    let mut ticker = tokio::time::interval(TICK);
    // This stream's own frame rate and bitrate (kept by the hub).
    let (mut frames, mut bytes, mut window) = (0u32, 0u64, Instant::now());
    loop {
        tokio::select! {
            item = opened.stream.next() => match item {
                None => return PumpEnd::Failed("the camera closed the stream".into()),
                Some(Err(e)) => return PumpEnd::Failed(format!("stream error: {e}")),
                Some(Ok(CodecItem::VideoFrame(f))) if f.stream_id() == opened.video => {
                    if f.has_new_parameters() || !matches!(*ch.state.borrow(), FeedState::Streaming(_)) {
                        match track_info(&opened, audio_track) {
                            Ok(Some(info)) => {
                                let info = Arc::new(info);
                                hub.facts.info(key, info.clone());
                                ch.state.send_replace(FeedState::Streaming(info));
                            }
                            Ok(None) => continue, // parameters not known yet
                            Err(e) => return PumpEnd::Failed(e),
                        }
                    }
                    let ts = f.timestamp();
                    let pts = rescale(ts.elapsed(), ts.clock_rate().get());
                    let keyframe = f.is_random_access_point();
                    let codec = match &*ch.state.borrow() {
                        FeedState::Streaming(info) => info.track.codec,
                        _ => continue,
                    };
                    let data = super::nal::prepare(f.into_data(), codec);
                    frames += 1;
                    bytes += data.len() as u64;
                    let frame = Frame { pts, keyframe, data: data.into() };
                    // Cache first, then send: a new recorder that subscribes and
                    // then snapshots can't miss a frame (duplicates are skipped by pts).
                    ch.preroll.push(&frame);
                    // No receivers is fine; the idle check handles it.
                    let _ = ch.frames.send(frame);
                }
                Some(Ok(CodecItem::AudioFrame(f))) if audio.as_ref().is_some_and(|(index, _, _)| f.stream_id() == *index) => {
                    let Some((_, rate, transcoder)) = audio.as_mut() else { continue };
                    let pts = rescale(f.timestamp().elapsed(), *rate);
                    for packet in transcoder.push(pts, f.data()) {
                        ch.preroll.push_audio(&packet);
                        let _ = ch.audio.send(packet);
                    }
                }
                Some(Ok(_)) => {}
            },
            _ = ch.reload.notified() => return PumpEnd::Reload,
            _ = ticker.tick() => {
                if idle.expired(hub, key, ch) {
                    return PumpEnd::Idle;
                }
                let elapsed = window.elapsed();
                if elapsed >= RATE_WINDOW {
                    let secs = elapsed.as_secs_f64();
                    hub.facts.rate(key, (f64::from(frames) / secs) as f32, (bytes as f64 * 8.0 / 1000.0 / secs).round() as u32);
                    (frames, bytes, window) = (0, 0, Instant::now());
                }
            }
        }
    }
}

fn track_info(opened: &Opened, audio: Option<super::audio::AudioTrack>) -> Result<Option<TrackInfo>, String> {
    let Some(ParametersRef::Video(v)) = opened.stream.streams()[opened.video].parameters() else { return Ok(None) };
    // Any codec is delivered (the supervisor reports it); live view and the
    // recorder refuse what they can't repackage.
    let codec = v.rfc6381_codec().to_string();
    let (width, height) = v.pixel_dimensions();
    let track = super::VideoTrack { codec: super::VideoCodec::from_rfc6381(&codec), width, height, decoder_config: v.extra_data().to_vec() };
    Ok(Some(TrackInfo { codec, audio_codec: opened.facts.audio_codec.clone(), audio, track }))
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
