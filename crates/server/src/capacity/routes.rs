//! `GET /api/v1/system/capacity`.

use axum::Json;
use axum::extract::State;
use chrono::Utc;
use watchgrid_model::{CameraStatus, CapacityEstimate, MotionSource, RecordingMode};

use super::estimate::{self, CameraLoad, Measured};
use super::hardware;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Recording share assumed until a camera has an hour of history.
fn assumed_duty(mode: RecordingMode) -> f32 {
    match mode {
        RecordingMode::Continuous => 1.0,
        RecordingMode::Events | RecordingMode::Scheduled => 0.3,
        RecordingMode::Manual => 0.05,
        RecordingMode::Disabled => 0.0,
    }
}

pub async fn estimate(State(s): State<AppState>) -> ApiResult<Json<CapacityEstimate>> {
    let root = s.recording_files.root();
    let hw = tokio::task::spawn_blocking(move || hardware::detect(&root)).await.map_err(ApiError::internal)?;
    let host = s.metrics.latest();

    // Recorded ms per camera over the last 24 hours: only the part of each
    // clip inside the window, plus clips still being written.
    let mut recorded: Vec<(String, i64)> = sqlx::query_as(
        "SELECT camera_id, COALESCE(SUM(EXTRACT(EPOCH FROM (LEAST(end_time, now()) - GREATEST(start_time, now() - interval '24 hours'))) * 1000), 0)::bigint
         FROM recordings WHERE end_time > now() - interval '24 hours' GROUP BY camera_id",
    )
    .fetch_all(&s.db)
    .await?;
    let now = Utc::now();
    for clip in s.recorder.live().all().iter().filter_map(|c| c.recording()) {
        let ms = (now - clip.start_time.max(now - chrono::Duration::hours(24))).num_milliseconds().max(0);
        match recorded.iter_mut().find(|(id, _)| *id == clip.camera_id) {
            Some((_, total)) => *total += ms,
            None => recorded.push((clip.camera_id, ms)),
        }
    }

    let cams: Vec<CameraLoad> = crate::cameras::list_live(&s)
        .await?
        .into_iter()
        .filter(|c| c.enabled)
        .map(|c| {
            let window = (now - c.created_at).num_milliseconds().clamp(0, 86_400_000) as f32;
            let observed = recorded.iter().find(|(id, _)| *id == c.id).map_or(0, |r| r.1) as f32;
            let duty = if window >= 3_600_000.0 { (observed / window).clamp(0.0, 1.0) } else { assumed_duty(c.recording.mode) };
            // Same rule as the recorder: the camera's ONVIF events or Watchgrid's software detection.
            let has_source = c.motion.enabled
                && match c.motion.source {
                    MotionSource::Onvif => c.onvif.as_ref().is_some_and(|o| !o.url.is_empty()),
                    MotionSource::Software => true,
                    MotionSource::Ai => false,
                };
            CameraLoad {
                online: c.status == CameraStatus::Online,
                main_kbps: c.main_stream.bitrate,
                duty,
                // Pre-record, the minimum-event delay and a small margin.
                buffer_secs: match c.recording.mode {
                    RecordingMode::Events if has_source => c.recording.pre_record_seconds + c.recording.min_event_seconds + 2,
                    RecordingMode::Continuous | RecordingMode::Scheduled => 3,
                    _ => 0,
                },
                continuous: c.recording.mode == RecordingMode::Continuous,
                events_without_source: c.recording.mode == RecordingMode::Events && !has_source,
                name: c.name,
            }
        })
        .collect();

    let measured = Measured { process_cpu: host.process_cpu, process_memory: host.process_memory };
    Ok(Json(estimate::estimate(hw, &cams, measured)))
}
