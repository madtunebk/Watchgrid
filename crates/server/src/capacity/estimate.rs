//! The estimate itself — pure, from measured numbers.

use watchgrid_model::{CapacityEstimate, Hardware, Resource, ResourceLoad};

/// Keep this share of the tightest resource free.
const HEADROOM: f32 = 0.8;
/// Share of each resource Watchgrid may plan to use.
const MEMORY_SHARE: f32 = 0.6;
const DISK_SHARE: f32 = 0.7;
const NET_SHARE: f32 = 0.7;
const MB: f32 = 1024.0 * 1024.0;

/// One camera as it behaves now.
#[derive(Debug, Clone)]
pub struct CameraLoad {
    pub name: String,
    pub online: bool,
    /// Measured main-stream bitrate (kbit/s), if connected.
    pub main_kbps: Option<u32>,
    /// Share of the time spent recording (0–1), observed or assumed.
    pub duty: f32,
    /// Seconds of video kept in RAM for pre-record.
    pub buffer_secs: u32,
    pub continuous: bool,
    /// Events mode without a motion source: it never records by itself.
    pub events_without_source: bool,
}

/// Watchgrid's own measured cost.
#[derive(Debug, Clone, Copy)]
pub struct Measured {
    /// Percent of one core.
    pub process_cpu: f32,
    pub process_memory: u64,
}

/// Assumed for cameras without a measurement yet (a typical 1080p stream).
const TYPICAL_KBPS: u32 = 4096;

struct PerCamera {
    cpu: f32,
    memory: f32,
    disk: f32,
    net: f32,
}

pub fn estimate(hw: Hardware, cams: &[CameraLoad], m: Measured) -> CapacityEstimate {
    let n = cams.len() as f32;
    let bps = |c: &CameraLoad| c.main_kbps.unwrap_or(TYPICAL_KBPS) as f32 * 1000.0;
    let buffers: f32 = cams.iter().map(|c| bps(c) / 8.0 * c.buffer_secs as f32).sum();
    let disk: f32 = cams.iter().map(|c| bps(c) / 8.0 * c.duty).sum();
    let net: f32 = cams.iter().map(bps).sum();

    let cpu_budget = hw.cpu_cores as f32 * 100.0;
    let mem_budget = hw.memory_total as f32 * MEMORY_SHARE;
    let disk_budget = hw.disk_write as f32 * DISK_SHARE;
    let net_budget = hw.network_link as f32 * NET_SHARE;
    let pct = |used: f32, budget: f32| if budget > 0.0 { (used / budget * 100.0).min(999.0) } else { 0.0 };

    let resources = vec![
        ResourceLoad {
            resource: Resource::Cpu,
            percent: pct(m.process_cpu, cpu_budget),
            detail: format!("{} · {} cores · Watchgrid uses {:.1}% of one core (no video decoding)", hw.cpu_model, hw.cpu_cores, m.process_cpu),
        },
        ResourceLoad {
            resource: Resource::Memory,
            percent: pct(m.process_memory as f32, mem_budget),
            detail: format!("Watchgrid uses {:.0} MB; pre-record buffers ≈ {:.0} MB", m.process_memory as f32 / MB, buffers / MB),
        },
        ResourceLoad {
            resource: Resource::DiskWrite,
            percent: pct(disk, disk_budget),
            detail: format!("≈ {:.2} MB/s average recording write; disk measured at {:.0} MB/s", disk / MB, hw.disk_write as f32 / MB),
        },
        ResourceLoad {
            resource: Resource::Network,
            percent: pct(net, net_budget),
            detail: format!("≈ {:.1} Mbit/s from cameras on a {} Mbit/s link", net / 1e6, hw.network_link / 1_000_000),
        },
    ];
    let worst = resources.iter().max_by(|a, b| a.percent.total_cmp(&b.percent)).map_or(Resource::Cpu, |r| r.resource);

    // Average cost of one more camera like the current ones (or a typical one).
    let per = if cams.is_empty() {
        PerCamera { cpu: 0.5, memory: TYPICAL_KBPS as f32 * 1000.0 / 8.0 * 9.0 + 2.0 * MB, disk: TYPICAL_KBPS as f32 * 1000.0 / 8.0 * 0.3, net: TYPICAL_KBPS as f32 * 1000.0 }
    } else {
        PerCamera { cpu: (m.process_cpu / n).max(0.2), memory: buffers / n + 2.0 * MB, disk: (disk / n).max(1.0), net: net / n }
    };
    let base_memory = (m.process_memory as f32 - buffers).max(10.0 * MB);
    let limits = [cpu_budget / per.cpu, (mem_budget - base_memory).max(0.0) / per.memory, disk_budget / per.disk, net_budget / per.net];
    let max_cameras = (limits.into_iter().fold(f32::MAX, f32::min) * HEADROOM).floor().clamp(0.0, 512.0) as u32;

    CapacityEstimate {
        advice: advice(cams, worst, resources[1].percent),
        hardware: hw,
        cameras: cams.len() as u32,
        max_cameras,
        bottleneck: worst,
        resources,
    }
}

/// "A, B, C and 5 more"
fn join_names(names: &[String]) -> String {
    const SHOWN: usize = 3;
    match names.len() {
        0..=SHOWN => names.join(", "),
        n => format!("{} and {} more", names[..SHOWN].join(", "), n - SHOWN),
    }
}

fn advice(cams: &[CameraLoad], bottleneck: Resource, memory_pct: f32) -> Vec<String> {
    let names = |f: &dyn Fn(&CameraLoad) -> bool| cams.iter().filter(|c| f(c)).map(|c| c.name.clone()).collect::<Vec<_>>();
    let mut out = Vec::new();
    let silent = names(&|c| c.events_without_source);
    if !silent.is_empty() {
        out.push(format!("{} use Events mode but have no motion source, so they never record by themselves.", join_names(&silent)));
    }
    let continuous = names(&|c| c.continuous);
    if !continuous.is_empty() {
        out.push(format!("{} record continuously. Events / motion mode typically writes far less.", join_names(&continuous)));
    }
    let unmeasured = names(&|c| c.main_kbps.is_none());
    if !unmeasured.is_empty() {
        out.push(format!("{}: no bitrate measured yet ({} kbit/s assumed).", join_names(&unmeasured), TYPICAL_KBPS));
    }
    let offline = names(&|c| !c.online);
    if !offline.is_empty() {
        out.push(format!("{} offline; the estimate assumes they come back.", join_names(&offline)));
    }
    if bottleneck == Resource::Memory && memory_pct > 70.0 {
        out.insert(0, "Memory is the limit: pre-record keeps seconds of video per camera in RAM. A shorter pre-record frees the most capacity.".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw() -> Hardware {
        Hardware { cpu_model: "Test CPU".into(), cpu_cores: 4, hw_decode: false, memory_total: 8 << 30, disk_write: 100_000_000, network_link: 1_000_000_000 }
    }

    fn cam(name: &str, kbps: u32, duty: f32, continuous: bool) -> CameraLoad {
        CameraLoad { name: name.into(), online: true, main_kbps: Some(kbps), duty, buffer_secs: 9, continuous, events_without_source: false }
    }

    #[test]
    fn disk_is_the_limit_for_continuous_recording() {
        // 3 cameras at 4 Mbit/s recording all the time: 1.5 MB/s of a 70 MB/s budget.
        let cams = [cam("a", 4000, 1.0, true), cam("b", 4000, 1.0, true), cam("c", 4000, 1.0, true)];
        let e = estimate(hw(), &cams, Measured { process_cpu: 3.0, process_memory: 40 << 20 });
        let disk = e.resources.iter().find(|r| r.resource == Resource::DiskWrite).unwrap();
        assert!((disk.percent - 2.14).abs() < 0.1, "{}", disk.percent);
        // Network: 70% of 1 Gbit / 4 Mbit = 175; disk: 70 MB/s / 0.5 MB/s = 140 → ×0.8 = 112.
        assert_eq!(e.max_cameras, 112);
        assert_eq!(e.bottleneck, Resource::DiskWrite);
        assert!(e.advice.iter().any(|a| a.contains("record continuously")));
    }

    #[test]
    fn works_without_cameras_and_flags_silent_ones() {
        let e = estimate(hw(), &[], Measured { process_cpu: 0.1, process_memory: 8 << 20 });
        assert!(e.max_cameras > 0);
        let mut silent = cam("ezviz", 8000, 0.0, false);
        silent.events_without_source = true;
        let e = estimate(hw(), &[silent], Measured { process_cpu: 1.0, process_memory: 20 << 20 });
        assert!(e.advice[0].contains("never record by themselves"));
    }
}
