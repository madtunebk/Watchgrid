//! What the machine has: CPU, memory, network link, disk write speed.
//! The disk is measured once (a short synced write) and remembered.

use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use watchgrid_model::Hardware;

const PROBE_BYTES: usize = 32 << 20;
/// Used when the disk can't be measured.
const DEFAULT_DISK: u64 = 100_000_000;
const DEFAULT_LINK: u64 = 1_000_000_000;

pub fn detect(recordings: &Path) -> Hardware {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    Hardware {
        cpu_model: cpu_model(&cpuinfo).unwrap_or_else(|| "Unknown CPU".into()),
        cpu_cores: cpu_cores(&cpuinfo).max(1),
        // Watchgrid never decodes video, so hardware decoding doesn't apply.
        hw_decode: false,
        memory_total: memory_total().unwrap_or(0),
        disk_write: disk_write(recordings),
        network_link: network_link().unwrap_or(DEFAULT_LINK),
    }
}

fn cpu_model(cpuinfo: &str) -> Option<String> {
    cpuinfo.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split_once(':')).map(|(_, v)| v.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn cpu_cores(cpuinfo: &str) -> u32 {
    let n = cpuinfo.lines().filter(|l| l.starts_with("processor")).count() as u32;
    if n > 0 { n } else { std::thread::available_parallelism().map_or(1, |n| n.get() as u32) }
}

fn memory_total() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find(|l| l.starts_with("MemTotal:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Fastest link among real network interfaces, in bits/s.
fn network_link() -> Option<u64> {
    let dir = std::fs::read_dir("/sys/class/net").ok()?;
    dir.filter_map(Result::ok)
        .filter(|e| e.file_name() != "lo")
        .filter_map(|e| std::fs::read_to_string(e.path().join("speed")).ok()?.trim().parse::<i64>().ok())
        .filter(|mbit| *mbit > 0)
        .max()
        .map(|mbit| mbit as u64 * 1_000_000)
}

/// Sequential write speed of the recordings volume, measured once.
fn disk_write(recordings: &Path) -> u64 {
    static MEASURED: OnceLock<u64> = OnceLock::new();
    *MEASURED.get_or_init(|| match measure(recordings) {
        Ok(bps) => bps,
        Err(e) => {
            tracing::warn!("cannot measure disk speed in {}: {e}", recordings.display());
            DEFAULT_DISK
        }
    })
}

fn measure(dir: &Path) -> std::io::Result<u64> {
    let path = dir.join(".partial").join(".capacity-probe");
    let chunk = vec![0x5au8; 1 << 20];
    let started = Instant::now();
    let result = (|| {
        let mut f = std::fs::File::create(&path)?;
        for _ in 0..PROBE_BYTES / chunk.len() {
            f.write_all(&chunk)?;
        }
        f.sync_all()
    })();
    let secs = started.elapsed().as_secs_f64().max(0.001);
    let _ = std::fs::remove_file(&path);
    result?;
    Ok((PROBE_BYTES as f64 / secs) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpuinfo() {
        let info = "processor\t: 0\nmodel name\t: Intel(R)   Celeron(R) J4125\nprocessor\t: 1\nmodel name\t: Intel(R) Celeron(R) J4125\n";
        assert_eq!(cpu_model(info).as_deref(), Some("Intel(R) Celeron(R) J4125"));
        assert_eq!(cpu_cores(info), 2);
    }

    #[test]
    fn disk_probe_writes_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("watchgrid-cap-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".partial")).unwrap();
        assert!(measure(&dir).unwrap() > 0);
        assert!(!dir.join(".partial/.capacity-probe").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
