//! Mock capacity estimate for a typical small NAS. The real backend will
//! measure the hardware and use observed stream costs; the shape of the
//! answer (per-resource load, max cameras, advice) stays the same.

use super::db::with_db;
use super::sim::latency;
use crate::api::{
    ApiResult, Camera, CameraStatus, CapacityEstimate, Hardware, MotionSource, RecordingMode, Resource, ResourceLoad,
    StreamRole,
};

const MB: u64 = 1024 * 1024;
/// Keep this much of every resource free.
const HEADROOM: f32 = 80.0;

fn hardware() -> Hardware {
    Hardware {
        cpu_model: "Intel Celeron J4125".into(),
        cpu_cores: 4,
        hw_decode: true,
        memory_total: 2048 * MB,
        disk_write: 120 * MB,
        network_link: 1_000_000_000,
    }
}

/// Per-camera cost in absolute units.
#[derive(Default, Clone, Copy)]
struct Cost {
    /// Percent of one core.
    cpu: f32,
    memory: f32,
    /// Bytes/s written on average.
    disk: f32,
    /// Bits/s received.
    net: f32,
}

impl std::ops::Add for Cost {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self { cpu: self.cpu + o.cpu, memory: self.memory + o.memory, disk: self.disk + o.disk, net: self.net + o.net }
    }
}

fn cost(c: &Camera, hw: &Hardware) -> Cost {
    let main_bps = c.main_stream.bitrate.unwrap_or(4096) as f32 * 1000.0;
    let sub_bps = c.sub_stream.as_ref().map(|s| s.bitrate.unwrap_or(512) as f32 * 1000.0);
    let motion_on_sub = c.motion.stream == StreamRole::Sub && sub_bps.is_some();

    // Ingest + remux is cheap; decoding for software motion is not.
    let mut cpu = 1.5;
    if c.motion.enabled {
        cpu += match c.motion.source {
            MotionSource::Onvif => 0.3,
            MotionSource::Software | MotionSource::Ai => {
                let decode = if motion_on_sub { 1.5 } else { 25.0 };
                if hw.hw_decode { decode / 3.0 } else { decode }
            }
        };
    }
    let duty = match c.recording.mode {
        RecordingMode::Continuous => 1.0,
        RecordingMode::Events | RecordingMode::Scheduled => 0.3,
        RecordingMode::Manual => 0.05,
        RecordingMode::Disabled => 0.0,
    };
    let net = main_bps + if motion_on_sub { sub_bps.unwrap_or(0.0) } else { 0.0 };
    Cost {
        cpu,
        memory: 25.0 * MB as f32 + main_bps / 8.0 * c.recording.pre_record_seconds as f32,
        disk: main_bps / 8.0 * duty,
        net,
    }
}

/// A typical 1080p camera with ONVIF motion and event recording.
fn typical() -> Cost {
    Cost { cpu: 1.8, memory: 27.5 * MB as f32, disk: 4096.0 * 1000.0 / 8.0 * 0.3, net: 4096.0 * 1000.0 }
}

pub async fn estimate() -> ApiResult<CapacityEstimate> {
    latency().await;
    let hw = hardware();
    Ok(with_db(|db| {
        let active: Vec<&Camera> = db.cameras.iter().filter(|c| c.enabled).collect();
        let total = active.iter().map(|c| cost(c, &hw)).fold(Cost::default(), |a, b| a + b);
        let base_memory = 180.0 * MB as f32;

        // Share of each resource Watchgrid may use.
        let cpu_budget = hw.cpu_cores as f32 * 100.0;
        let mem_budget = hw.memory_total as f32 * 0.6;
        let disk_budget = hw.disk_write as f32 * 0.7;
        let net_budget = hw.network_link as f32 * 0.7;

        let pct = |used: f32, budget: f32| (used / budget * 100.0).min(999.0);
        let resources = vec![
            ResourceLoad {
                resource: Resource::Cpu,
                percent: pct(total.cpu, cpu_budget),
                detail: format!(
                    "{} cores{} · {} camera(s) decoded for software motion",
                    hw.cpu_cores,
                    if hw.hw_decode { ", hardware decoding" } else { "" },
                    active.iter().filter(|c| c.motion.enabled && c.motion.source != MotionSource::Onvif).count()
                ),
            },
            ResourceLoad {
                resource: Resource::Memory,
                percent: pct(base_memory + total.memory, mem_budget),
                detail: "Pre-record buffers and per-camera state".into(),
            },
            ResourceLoad {
                resource: Resource::DiskWrite,
                percent: pct(total.disk, disk_budget),
                detail: format!("≈ {:.1} MB/s average recording write", total.disk / MB as f32),
            },
            ResourceLoad {
                resource: Resource::Network,
                percent: pct(total.net, net_budget),
                detail: format!("≈ {:.0} Mbit/s from cameras", total.net / 1e6),
            },
        ];

        let worst = resources.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)).expect("four resources");
        let n = active.len() as f32;
        // Scale the current mix up (or a typical camera if there are none) until the first resource hits the headroom line.
        let per_camera = if active.is_empty() { typical() } else { Cost { cpu: total.cpu / n, memory: total.memory / n, disk: total.disk / n, net: total.net / n } };
        let limits = [
            cpu_budget / per_camera.cpu,
            (mem_budget - base_memory) / per_camera.memory,
            disk_budget / per_camera.disk.max(1.0),
            net_budget / per_camera.net,
        ];
        let max_cameras = (limits.iter().cloned().fold(f32::MAX, f32::min) * HEADROOM / 100.0).floor().clamp(0.0, 256.0) as u32;

        CapacityEstimate {
            hardware: hw.clone(),
            cameras: active.len() as u32,
            max_cameras,
            bottleneck: worst.resource,
            advice: advice(&active, worst.resource, resources[1].percent),
            resources,
        }
    }))
}

/// "A, B, C and 5 more"
fn join_names(names: &[String]) -> String {
    const SHOWN: usize = 3;
    match names.len() {
        0..=SHOWN => names.join(", "),
        n => format!("{} and {} more", names[..SHOWN].join(", "), n - SHOWN),
    }
}

fn advice(active: &[&Camera], bottleneck: Resource, memory_pct: f32) -> Vec<String> {
    let names = |f: &dyn Fn(&Camera) -> bool| active.iter().filter(|c| f(c)).map(|c| c.name.clone()).collect::<Vec<_>>();
    let software_main = |c: &Camera| {
        c.motion.enabled && c.motion.source == MotionSource::Software && !(c.motion.stream == StreamRole::Sub && c.sub_stream.is_some())
    };
    let mut out = Vec::new();

    let with_sub = names(&|c| software_main(c) && c.sub_stream.is_some());
    if !with_sub.is_empty() {
        out.push(format!("Use the substream for motion detection on {} — about 15× less CPU each.", join_names(&with_sub)));
    }
    let without_sub = names(&|c| software_main(c) && c.sub_stream.is_none() && c.onvif.is_some());
    if !without_sub.is_empty() {
        out.push(format!("Switch {} to camera (ONVIF) motion detection; they already support it.", join_names(&without_sub)));
    }
    let continuous = names(&|c| c.recording.mode == RecordingMode::Continuous);
    if !continuous.is_empty() {
        out.push(format!(
            "{} record continuously. Events / motion mode typically writes about 70 % less.",
            join_names(&continuous)
        ));
    }
    let offline = names(&|c| c.status == CameraStatus::Offline);
    if !offline.is_empty() {
        out.push(format!("{} offline; the estimate assumes they come back.", join_names(&offline)));
    }
    if bottleneck == Resource::Memory && memory_pct > 70.0 {
        out.insert(0, "Memory is the limit: pre-record buffers keep seconds of video per camera in RAM. A shorter pre-record or more RAM frees the most capacity.".into());
    }
    out
}
