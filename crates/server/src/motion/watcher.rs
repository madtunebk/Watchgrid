//! One detector per camera whose motion source is "software": it follows
//! the camera's substream from the media hub, decodes it on a low-priority
//! thread and publishes DetectionStarted/Ended like the ONVIF watchers do.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use watchgrid_model::{EventType, MotionSource, MotionZone};

use super::analyzer::{Analyzer, Transition, cells};
use super::decoder::H264;
use crate::bus::{Bus, BusEvent};
use crate::live::LiveRegistry;
use crate::media::{FeedState, Frame, MediaHub, StreamKind};

/// Reported as the detection's topic (the events journal keeps it).
pub const TOPIC: &str = "Watchgrid/SoftwareMotion";
/// Analyse at most this often; decoding still sees every frame.
const ANALYSE_EVERY: i64 = 90_000 / 5;
/// Frames waiting for the decoder; beyond this it is behind and skips
/// ahead to the next keyframe instead of falling further back.
const QUEUE: usize = 32;
const RETRY: Duration = Duration::from_secs(5);
/// No video for this long ends a detection in progress: its end is judged
/// on stream time, which stops when the camera does.
const STALL: Duration = Duration::from_secs(5);
/// Diagnostics every this many analysed pictures (≈ 10 s).
const STATS_EVERY: u32 = 50;

#[derive(Clone)]
pub struct Deps {
    pub db: PgPool,
    pub hub: Arc<MediaHub>,
    pub live: Arc<LiveRegistry>,
    pub bus: Bus,
}

struct Running {
    task: JoinHandle<()>,
    /// A detection is in progress (closed if the task is replaced).
    active: Arc<AtomicBool>,
}

pub struct Detectors {
    deps: Deps,
    tasks: Mutex<HashMap<String, Running>>,
    /// Tests never decode video.
    enabled: bool,
}

impl Detectors {
    pub fn new(deps: Deps) -> Self {
        Self { deps, tasks: Mutex::new(HashMap::new()), enabled: true }
    }

    #[cfg(test)]
    pub fn inert(deps: Deps) -> Self {
        Self { enabled: false, ..Self::new(deps) }
    }

    /// (Re)start after the camera was added or changed; the task decides
    /// from the settings whether there is anything to do.
    pub fn apply(&self, id: &str, exists: bool) {
        if let Some(old) = self.tasks.lock().expect("detectors lock").remove(id) {
            old.task.abort();
            if old.active.load(Ordering::Relaxed) {
                end(&self.deps, id);
            }
        }
        if exists && self.enabled {
            let active = Arc::new(AtomicBool::new(false));
            let task = tokio::spawn(run(self.deps.clone(), id.to_string(), active.clone()));
            self.tasks.lock().expect("detectors lock").insert(id.to_string(), Running { task, active });
        }
    }
}

/// What the detector needs from the camera's settings.
struct Settings {
    sensitivity: u8,
    zones: Vec<MotionZone>,
}

async fn settings(db: &PgPool, id: &str) -> Result<Option<Settings>, String> {
    let camera = crate::cameras::repo_get(db, id).await.map_err(|e| e.to_string())?;
    Ok(camera.filter(|c| c.enabled && c.motion.enabled && c.motion.source == MotionSource::Software).map(|c| Settings { sensitivity: c.motion.sensitivity, zones: c.motion.zones }))
}

async fn run(deps: Deps, id: String, active: Arc<AtomicBool>) {
    let cfg = loop {
        match settings(&deps.db, &id).await {
            Ok(Some(cfg)) => break cfg,
            Ok(None) => return, // gone, disabled, or motion from elsewhere
            Err(e) => {
                tracing::warn!(camera = %id, "cannot load motion settings: {e}");
                tokio::time::sleep(RETRY).await;
            }
        }
    };
    tracing::info!(camera = %id, "software motion detection started");
    loop {
        follow(&deps, &id, &cfg, &active).await;
        if active.swap(false, Ordering::Relaxed) {
            end(&deps, &id);
        }
        tokio::time::sleep(RETRY).await;
    }
}

/// Messages to the decoding thread.
enum Work {
    Track(Vec<u8>),
    Frame(Frame),
    /// The detection was closed because the stream stalled.
    Forget,
}

/// Feed the substream to a decoding thread until the feed ends.
async fn follow(deps: &Deps, id: &str, cfg: &Settings, active: &AtomicBool) {
    // The hub serves the main stream when a camera has no substream.
    let mut sub = deps.hub.subscribe(id, StreamKind::Sub);
    // The feed may already be streaming: look at its current state first.
    sub.state.mark_changed();
    let (work, queue) = sync_channel::<Work>(QUEUE);
    let (report, mut transitions) = unbounded_channel();
    let (sensitivity, zones, camera) = (cfg.sensitivity, cfg.zones.clone(), id.to_string());
    let thread = std::thread::Builder::new()
        .name(format!("motion-{id}"))
        .spawn(move || decode_loop(&camera, queue, Analyzer::new(sensitivity, &zones), report));
    if let Err(e) = thread {
        tracing::error!(camera = %id, "cannot start motion detection: {e}");
        return;
    }
    let mut avcc: Option<Vec<u8>> = None;
    let mut unsupported_logged = false;
    // After a drop the decoder can only resume at a keyframe.
    let mut need_keyframe = true;
    let mut last_frame = Instant::now();
    let mut forget = false;
    let mut watchdog = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = watchdog.tick() => {
                if active.load(Ordering::Relaxed) && last_frame.elapsed() >= STALL {
                    tracing::debug!(camera = %id, "no video for {} s: software motion ended", STALL.as_secs());
                    active.store(false, Ordering::Relaxed);
                    end(deps, id);
                    forget = true;
                }
                // Never block here: retried each second until there is room.
                if forget {
                    match work.try_send(Work::Forget) {
                        Ok(()) => forget = false,
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
            }
            changed = sub.state.changed() => {
                if changed.is_err() {
                    return;
                }
                let FeedState::Streaming(info) = sub.state.borrow_and_update().clone() else { continue };
                if !info.is_h264() {
                    if !unsupported_logged {
                        tracing::warn!(camera = %id, codec = %info.codec, "software motion detection needs H.264; this stream is not");
                        unsupported_logged = true;
                    }
                    avcc = None;
                    continue;
                }
                if avcc.as_deref() != Some(&info.track.avcc) {
                    avcc = Some(info.track.avcc.clone());
                    need_keyframe = true;
                    if work.send(Work::Track(info.track.avcc.clone())).is_err() {
                        return;
                    }
                }
            }
            frame = sub.frames.recv() => match frame {
                Ok(frame) => {
                    last_frame = Instant::now();
                    if avcc.is_none() || (need_keyframe && !frame.keyframe) {
                        continue;
                    }
                    need_keyframe = false;
                    match work.try_send(Work::Frame(frame)) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => need_keyframe = true,
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
                Err(RecvError::Lagged(_)) => need_keyframe = true,
                Err(RecvError::Closed) => return,
            },
            transition = transitions.recv() => match transition {
                Some(Transition::Started) => {
                    tracing::debug!(camera = %id, "software motion started");
                    active.store(true, Ordering::Relaxed);
                    deps.live.update(id, |l| l.motion_active = true);
                    deps.bus.publish(BusEvent::DetectionStarted { camera_id: id.to_string(), kind: EventType::Motion, topic: TOPIC.into(), at: Utc::now() });
                }
                Some(Transition::Ended) => {
                    tracing::debug!(camera = %id, "software motion ended");
                    active.store(false, Ordering::Relaxed);
                    end(deps, id);
                }
                None => return, // the thread gave up
            },
        }
    }
}

fn end(deps: &Deps, id: &str) {
    deps.live.update(id, |l| l.motion_active = false);
    deps.bus.publish(BusEvent::DetectionEnded { camera_id: id.to_string(), kind: EventType::Motion, at: Utc::now() });
}

/// Decode frames and analyse pictures until the sender goes away.
fn decode_loop(id: &str, queue: Receiver<Work>, mut analyzer: Analyzer, report: UnboundedSender<Transition>) {
    lower_priority();
    let mut decoder: Option<H264> = None;
    let mut last_analysed: Option<i64> = None;
    let mut errors = 0u32;
    let mut pictures = 0u32;
    while let Ok(work) = queue.recv() {
        let frame = match work {
            Work::Track(avcc) => {
                decoder = match H264::new(&avcc) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        tracing::warn!(camera = %id, "motion detection: {e}");
                        None
                    }
                };
                analyzer.reset();
                last_analysed = None;
                continue;
            }
            Work::Frame(frame) => frame,
            Work::Forget => {
                analyzer.forget();
                last_analysed = None;
                continue;
            }
        };
        let Some(h264) = decoder.as_mut() else { continue };
        match h264.decode(&frame.data, |p| cells(p.y, p.width, p.height, p.stride)) {
            Ok(Some(grid)) => {
                errors = 0;
                if last_analysed.is_some_and(|t| frame.pts < t) {
                    // The camera session restarted and with it the clock.
                    analyzer.reset();
                    last_analysed = None;
                }
                if last_analysed.is_some_and(|t| frame.pts - t < ANALYSE_EVERY) {
                    continue;
                }
                last_analysed = Some(frame.pts);
                pictures += 1;
                if pictures.is_multiple_of(STATS_EVERY) {
                    let (peak, trigger) = analyzer.take_peak();
                    tracing::debug!(camera = %id, "motion: {pictures} pictures analysed, largest change {:.2}% (triggers at {:.2}%)", peak * 100.0, trigger * 100.0);
                }
                if let Some(t) = analyzer.push(&grid, frame.pts as f64 / 90_000.0) {
                    if report.send(t).is_err() {
                        return;
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                errors += 1;
                if errors == 1 || errors.is_multiple_of(500) {
                    tracing::debug!(camera = %id, "motion detection: cannot decode a frame: {e}");
                }
            }
        }
    }
}

/// Recording and live video matter more than motion analysis: run the
/// decoder at a lower CPU priority (nice 10, this thread only).
fn lower_priority() {
    // SAFETY: plain syscalls on the calling thread.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::id_t;
        libc::setpriority(libc::PRIO_PROCESS, tid, 10);
    }
}
