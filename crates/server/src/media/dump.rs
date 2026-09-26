//! Diagnostics: write a camera's live stream to a file exactly as the
//! browser receives it (fMP4 init + fragments), plus per-frame timing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::error::RecvError;

use super::fragmenter::Fragmenter;
use super::{FeedState, MediaHub, StreamKind, fmp4};

pub async fn run(hub: &Arc<MediaHub>, camera_id: &str, kind: StreamKind, seconds: u64, out: &str) -> Result<(), String> {
    let mut sub = hub.subscribe(camera_id, kind);
    let deadline = Instant::now() + Duration::from_secs(20);
    let info = loop {
        if let FeedState::Streaming(info) = &*sub.state.borrow_and_update() {
            break info.clone();
        }
        if Instant::now() > deadline {
            return Err(format!("no video: {:?}", *sub.state.borrow()));
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), sub.state.changed()).await;
    };
    println!("codec {} {}x{}, audio {}", info.codec, info.track.width, info.track.height, if info.audio.is_some() { "opus" } else { "none" });
    let mut file = tokio::fs::File::create(out).await.map_err(|e| e.to_string())?;
    file.write_all(&fmp4::init_segment(&info.track, info.audio.as_ref())).await.map_err(|e| e.to_string())?;
    let with_audio = info.audio.is_some();
    let mut audio_packets = 0u32;

    let mut fragmenter = Fragmenter::default();
    let (mut frames, mut keys, mut segments, mut lagged) = (0u32, 0u32, 0u32, 0u64);
    let (mut last_pts, mut bad_deltas) = (None::<i64>, 0u32);
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        let next = tokio::select! {
            f = sub.frames.recv() => Ok(f),
            a = sub.audio.recv(), if with_audio => Err(a),
            () = tokio::time::sleep(Duration::from_secs(1)) => continue,
        };
        let frame = match next {
            Ok(f) => f,
            Err(Ok(packet)) => {
                if let Some(seg) = fragmenter.push_audio(&packet) {
                    audio_packets += 1;
                    file.write_all(&seg).await.map_err(|e| e.to_string())?;
                }
                continue;
            }
            Err(Err(_)) => continue,
        };
        match Ok::<_, ()>(frame) {
            Ok(Ok(frame)) => {
                frames += 1;
                keys += u32::from(frame.keyframe);
                if last_pts.is_some_and(|p| frame.pts <= p) {
                    bad_deltas += 1;
                }
                last_pts = Some(frame.pts);
                if let Some(seg) = fragmenter.push(frame) {
                    segments += 1;
                    file.write_all(&seg).await.map_err(|e| e.to_string())?;
                }
            }
            Ok(Err(RecvError::Lagged(n))) => lagged += n,
            Ok(Err(RecvError::Closed)) => break,
            Err(()) => {}
        }
    }
    println!("{frames} frames, {keys} keyframes, {segments} fragments written, {audio_packets} audio packets, {bad_deltas} non-increasing timestamps, {lagged} frames lagged");
    println!("wrote {out}");
    Ok(())
}
