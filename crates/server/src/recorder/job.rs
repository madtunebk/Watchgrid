//! One manual recording, from STARTING to back to IDLE.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::time::Instant;
use watchgrid_model::RecordingReason;

use super::writer::{Finished, Mp4Writer};
use super::{Deps, Phase};
use crate::bus::BusEvent;
use crate::media::{FeedState, StreamKind, Subscription, TrackInfo};
use crate::recordings::{self, NewRecording, RecordingFiles};

/// Time allowed to get the stream and its first keyframe.
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// No frames for this long counts as a disconnect.
const STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Hard cap per file (MP4 v0 durations overflow after ~13 h at 90 kHz).
const MAX_LENGTH: Duration = Duration::from_secs(12 * 3600);

/// Why recording ended.
#[derive(Debug)]
enum End {
    Stopped,
    Failed(String),
}

pub async fn run(deps: Deps, camera_id: String, mut stop: watch::Receiver<bool>, phase: watch::Sender<Phase>) {
    let started_at = Utc::now();
    let id = format!("rec-{camera_id}-{}", started_at.format("%Y%m%d-%H%M%S%3f"));
    let result = record(&deps, &camera_id, &id, &mut stop, &phase).await;
    let (recording_id, error) = match result {
        Ok(recording_id) => (recording_id, None),
        Err(e) => {
            tracing::warn!(camera = %camera_id, recording = %id, "recording failed: {e}");
            (None, Some(e))
        }
    };
    phase.send_replace(Phase::Idle);
    deps.bus.publish(BusEvent::RecordingStopped { camera_id, recording_id, error });
}

/// Returns the published recording id (None if nothing was recorded).
async fn record(deps: &Deps, camera_id: &str, id: &str, stop: &mut watch::Receiver<bool>, phase: &watch::Sender<Phase>) -> Result<Option<String>, String> {
    let mut sub = deps.hub.subscribe(camera_id, StreamKind::Main);
    let track = match wait_for_track(&mut sub, stop).await {
        Ok(track) => track,
        Err(End::Stopped) => return Ok(None),
        Err(End::Failed(e)) => return Err(e),
    };
    let partial = deps.files.partial_path(id);
    let mut writer = Mp4Writer::create(partial.clone()).await.map_err(|e| format!("cannot create {}: {e}", partial.display()))?;

    let (end, first_frame_at) = capture(deps, camera_id, &track, &mut sub, &mut writer, stop, phase).await;
    drop(sub); // let the live feed close if nobody else watches
    phase.send_replace(Phase::Finalizing);

    let Some(start_time) = first_frame_at.filter(|_| writer.samples() > 0) else {
        discard(writer.path()).await;
        return match end {
            End::Stopped => Ok(None),
            End::Failed(e) => Err(e),
        };
    };
    let finished = match writer.finish(&track.track).await {
        Ok(f) => f,
        Err(e) => {
            discard(&partial).await;
            return Err(format!("cannot finalize the recording: {e}"));
        }
    };
    let published = publish(deps, camera_id, id, &track, start_time, finished).await?;
    if let End::Failed(e) = end {
        // The footage up to the failure is saved; still report why it stopped.
        tracing::warn!(camera = %camera_id, recording = %id, "recording ended early: {e}");
    }
    Ok(Some(published))
}

/// Wait until the feed delivers video (its codec setup is known).
async fn wait_for_track(sub: &mut Subscription, stop: &mut watch::Receiver<bool>) -> Result<Arc<TrackInfo>, End> {
    let deadline = Instant::now() + START_TIMEOUT;
    let mut last_problem = None;
    loop {
        if let FeedState::Streaming(info) = &*sub.state.borrow_and_update() {
            return Ok(info.clone());
        }
        if let FeedState::Failed(e) = &*sub.state.borrow() {
            last_problem = Some(e.clone());
        }
        tokio::select! {
            _ = stop.changed() => return Err(End::Stopped),
            changed = sub.state.changed() => if changed.is_err() {
                return Err(End::Failed("live feed ended".into()));
            },
            _ = tokio::time::sleep_until(deadline) => {
                return Err(End::Failed(last_problem.unwrap_or_else(|| "the camera did not start streaming".into())));
            }
        }
    }
}

/// Copy frames into the file until STOP or a failure.
/// Returns why it ended and the wall-clock time of the first written frame.
async fn capture(
    deps: &Deps,
    camera_id: &str,
    track: &TrackInfo,
    sub: &mut Subscription,
    writer: &mut Mp4Writer,
    stop: &mut watch::Receiver<bool>,
    phase: &watch::Sender<Phase>,
) -> (End, Option<chrono::DateTime<Utc>>) {
    let mut first_frame_at = None;
    let start_deadline = Instant::now() + START_TIMEOUT;
    let mut last_frame = Instant::now();
    let mut recording_since = None::<Instant>;
    loop {
        let stall_at = if recording_since.is_some() { last_frame + STALL_TIMEOUT } else { start_deadline };
        let end = tokio::select! {
            _ = stop.changed() => Some(End::Stopped),
            changed = sub.state.changed() => match changed {
                Err(_) => Some(End::Failed("live feed ended".into())),
                Ok(()) => match &*sub.state.borrow_and_update() {
                    FeedState::Streaming(info) if info.track == track.track => None,
                    FeedState::Streaming(_) => Some(End::Failed("the camera changed its video settings".into())),
                    FeedState::Failed(e) => Some(End::Failed(format!("camera disconnected: {e}"))),
                    FeedState::Connecting => Some(End::Failed("camera disconnected".into())),
                },
            },
            frame = sub.frames.recv() => match frame {
                Ok(frame) => match writer.push(frame).await {
                    Ok(true) => {
                        last_frame = Instant::now();
                        if recording_since.is_none() {
                            recording_since = Some(last_frame);
                            first_frame_at = Some(Utc::now());
                            phase.send_replace(Phase::Recording);
                            tracing::info!(camera = %camera_id, "recording started");
                            deps.bus.publish(BusEvent::RecordingStarted { camera_id: camera_id.to_string() });
                        }
                        None
                    }
                    Ok(false) => None,
                    Err(e) => Some(End::Failed(write_error(e))),
                },
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(camera = %camera_id, "recorder fell behind, skipped {n} frames");
                    writer.resync().await.err().map(|e| End::Failed(write_error(e)))
                }
                Err(RecvError::Closed) => Some(End::Failed("live feed ended".into())),
            },
            _ = tokio::time::sleep_until(stall_at) => Some(End::Failed(
                if recording_since.is_some() { "no video from the camera".into() } else { "no keyframe from the camera".into() }
            )),
        };
        let end = end.or_else(|| recording_since.filter(|s| s.elapsed() >= MAX_LENGTH).map(|_| End::Stopped));
        if let Some(end) = end {
            return (end, first_frame_at);
        }
    }
}

fn write_error(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::StorageFull {
        "the disk is full".into()
    } else {
        format!("write failed: {e}")
    }
}

/// Move the file into place and record it in the database.
async fn publish(deps: &Deps, camera_id: &str, id: &str, track: &TrackInfo, start_time: chrono::DateTime<Utc>, f: Finished) -> Result<String, String> {
    let relative = RecordingFiles::final_relative(camera_id, id, start_time);
    let path = deps.files.publish(&f.path, &relative).await.map_err(|e| {
        format!("cannot move the recording into place: {e} (kept at {})", f.path.display())
    })?;
    let duration_ms = (f.duration * 1000 / u64::from(crate::media::TIMESCALE)) as i64;
    let row = NewRecording {
        id: id.to_string(),
        camera_id: camera_id.to_string(),
        reason: RecordingReason::Manual,
        start_time,
        end_time: start_time + chrono::Duration::milliseconds(duration_ms),
        duration_ms,
        file_size: f.size as i64,
        path: relative,
        codec: track.codec.clone(),
        width: track.track.width as i32,
        height: track.track.height as i32,
    };
    // The file is complete either way; a failed insert must not delete footage.
    recordings::insert(&deps.db, &row).await.map_err(|e| format!("recording saved to {} but not indexed: {e}", path.display()))?;
    tracing::info!(camera = %camera_id, recording = %id, frames = f.samples, seconds = duration_ms / 1000, bytes = f.size, "recording finalized");
    Ok(id.to_string())
}

async fn discard(path: &std::path::Path) {
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("cannot remove {}: {e}", path.display());
    }
}
