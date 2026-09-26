//! How much the server can take: projected load per resource and the
//! estimated number of cameras it can handle with the current settings.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// Decoding for software motion detection (the main CPU consumer).
    Cpu,
    /// Pre-record buffers and per-camera state.
    Memory,
    /// Writing recordings.
    DiskWrite,
    /// Camera streams in + live view out.
    Network,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLoad {
    pub resource: Resource,
    /// Projected steady-state use, 0-100 % of what the server can sustain.
    pub percent: f32,
    /// Human-readable basis, e.g. "3 of 4 cores · 2 cameras decoded in software".
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub cpu_model: String,
    pub cpu_cores: u32,
    /// Hardware video decoding available (VAAPI / Quick Sync).
    pub hw_decode: bool,
    pub memory_total: u64,
    /// Sustained write throughput of the recording volume, bytes/s.
    pub disk_write: u64,
    /// Network link speed, bits/s.
    pub network_link: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapacityEstimate {
    pub hardware: Hardware,
    pub cameras: u32,
    /// Estimated cameras this server can run with a similar mix of settings,
    /// keeping ~20 % headroom.
    pub max_cameras: u32,
    /// The resource that runs out first.
    pub bottleneck: Resource,
    pub resources: Vec<ResourceLoad>,
    /// Concrete suggestions to free capacity.
    pub advice: Vec<String>,
}
