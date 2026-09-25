//! Host metrics from Linux `/proc`, sampled in the background so requests
//! only read the latest numbers.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HostMetrics {
    /// Percent of all CPUs.
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// Watchgrid's own resident memory.
    pub process_memory: u64,
    /// Bytes per second, all interfaces except loopback.
    pub rx: u64,
    pub tx: u64,
}

#[derive(Clone, Default)]
pub struct Sampler(Arc<RwLock<HostMetrics>>);

impl Sampler {
    pub fn latest(&self) -> HostMetrics {
        *self.0.read().expect("metrics lock")
    }

    pub async fn run(self) {
        let mut prev_cpu = read_cpu();
        let mut prev_net = (read_net(), Instant::now());
        loop {
            tokio::time::sleep(EVERY).await;
            let cpu = read_cpu();
            let net = (read_net(), Instant::now());
            let (used, total) = read_memory().unwrap_or((0, 0));
            let secs = net.1.duration_since(prev_net.1).as_secs_f64().max(0.001);
            let rate = |now: u64, before: u64| (now.saturating_sub(before) as f64 / secs) as u64;
            let m = HostMetrics {
                cpu: cpu_percent(prev_cpu, cpu),
                memory_used: used,
                memory_total: total,
                process_memory: read_process_memory().unwrap_or(0),
                rx: rate(net.0.0, prev_net.0.0),
                tx: rate(net.0.1, prev_net.0.1),
            };
            *self.0.write().expect("metrics lock") = m;
            (prev_cpu, prev_net) = (cpu, net);
        }
    }
}

/// (busy, total) jiffies from the aggregate `cpu` line.
fn read_cpu() -> (u64, u64) {
    std::fs::read_to_string("/proc/stat").ok().and_then(|s| parse_cpu(&s)).unwrap_or((0, 0))
}

fn parse_cpu(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    let total: u64 = v.iter().take(8).sum();
    let idle = v.get(3)? + v.get(4).unwrap_or(&0); // idle + iowait
    Some((total - idle, total))
}

fn cpu_percent(before: (u64, u64), now: (u64, u64)) -> f32 {
    let total = now.1.saturating_sub(before.1);
    if total == 0 { 0.0 } else { now.0.saturating_sub(before.0) as f32 / total as f32 * 100.0 }
}

/// Resident memory of this process, from `/proc/self/status` (VmRSS, kB).
fn read_process_memory() -> Option<u64> {
    parse_rss(&std::fs::read_to_string("/proc/self/status").ok()?)
}

fn parse_rss(status: &str) -> Option<u64> {
    let kb: u64 = status.lines().find(|l| l.starts_with("VmRSS:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// (used, total) bytes; "used" excludes reclaimable cache.
fn read_memory() -> Option<(u64, u64)> {
    parse_memory(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

fn parse_memory(info: &str) -> Option<(u64, u64)> {
    let kb = |key: &str| info.lines().find(|l| l.starts_with(key))?.split_whitespace().nth(1)?.parse::<u64>().ok();
    let (total, available) = (kb("MemTotal:")?, kb("MemAvailable:")?);
    Some(((total - available) * 1024, total * 1024))
}

/// Total (rx, tx) bytes of non-loopback interfaces.
fn read_net() -> (u64, u64) {
    std::fs::read_to_string("/proc/net/dev").map(|s| parse_net(&s)).unwrap_or((0, 0))
}

fn parse_net(dev: &str) -> (u64, u64) {
    dev.lines()
        .filter_map(|l| l.split_once(':'))
        .filter(|(name, _)| name.trim() != "lo")
        .filter_map(|(_, rest)| {
            let v: Vec<u64> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            Some((*v.first()?, *v.get(8)?))
        })
        .fold((0, 0), |(r, t), (a, b)| (r + a, t + b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_formats() {
        assert_eq!(parse_cpu("cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 1 2 3 4\n"), Some((200, 1000)));
        assert_eq!(cpu_percent((200, 1000), (300, 1200)), 50.0);
        assert_eq!(parse_memory("MemTotal:  1000 kB\nMemFree: 1 kB\nMemAvailable:  250 kB\n"), Some((750 * 1024, 1000 * 1024)));
        let dev = "Inter-| Receive\n face |bytes\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 100 1 0 0 0 0 0 0 40 1 0 0 0 0 0 0\n  wg0: 5 1 0 0 0 0 0 0 6 1 0 0 0 0 0 0\n";
        assert_eq!(parse_net(dev), (105, 46));
        assert_eq!(parse_rss("Name:\twatchgrid\nVmRSS:\t   7788 kB\n"), Some(7788 * 1024));
    }
}
