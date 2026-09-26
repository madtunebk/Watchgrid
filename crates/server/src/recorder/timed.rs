//! Continuous and scheduled recording: record while wanted (always, or
//! inside the camera's schedule windows), in clips of "maximum clip
//! duration". Each new clip starts at the last buffered keyframe, so
//! consecutive clips leave no gap. A recording that ends on its own is
//! retried with backoff.

use std::time::Duration;

use tokio::time::Instant;
use watchgrid_model::{Camera, RecordingMode, RecordingReason, schedule_active};

use super::Spec;
use super::auto::{Controller, Plan};
use crate::supervisor::backoff;

const TICK: Duration = Duration::from_secs(5);
const OWN: [RecordingReason; 2] = [RecordingReason::Continuous, RecordingReason::Scheduled];

pub(super) async fn run(ctl: &Controller, camera: &Camera) {
    let plan = Plan::from_settings(&camera.recording);
    let reason = if camera.recording.mode == RecordingMode::Continuous { RecordingReason::Continuous } else { RecordingReason::Scheduled };
    // A few seconds of buffer let a new clip start at the last keyframe.
    let _keepalive = ctl.hub.subscribe_with_preroll(&ctl.id, plan.stream, 3);
    let mut clip_started: Option<Instant> = None;
    let mut attempt = 0u32;
    let mut retry_at = Instant::now();
    let mut ticker = tokio::time::interval(TICK);

    loop {
        ticker.tick().await;
        let want = match reason {
            RecordingReason::Continuous => true,
            _ => match crate::settings::local_clock(&ctl.db).await {
                Ok((day, minute)) => schedule_active(&camera.recording.schedule, day, minute),
                Err(e) => {
                    tracing::warn!(camera = %ctl.id, "cannot read the local time: {e}");
                    continue;
                }
            },
        };
        let running = ctl.recorder.running_reason(&ctl.id);
        let mine = running.is_some_and(|r| OWN.contains(&r));

        if !want {
            if mine {
                tracing::info!(camera = %ctl.id, "schedule window ended");
                ctl.recorder.stop_if(&ctl.id, &OWN).await;
            }
            clip_started = None;
            continue;
        }
        if mine {
            // A clip already running when this controller started (e.g. after
            // a settings edit): count its length from now, so it's still cut.
            let started = *clip_started.get_or_insert_with(Instant::now);
            if started.elapsed() < plan.max_clip {
                attempt = 0;
                continue;
            }
            // Clip is long enough: close it and start the next one right away.
            ctl.recorder.stop_if(&ctl.id, &OWN).await;
            clip_started = None;
        } else if running.is_some() {
            continue; // a manual or event recording is running; leave it be
        } else if clip_started.take().is_some() {
            // Our recording ended by itself (camera down, disk…): back off.
            attempt += 1;
            retry_at = Instant::now() + backoff::delay(attempt);
        }
        if Instant::now() < retry_at {
            continue;
        }
        if ctl.recorder.start_with(&ctl.id, Spec { reason, stream: plan.stream, preroll_secs: 1 }) {
            clip_started = Some(Instant::now());
        }
    }
}
