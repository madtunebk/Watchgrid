//! Server health: host metrics, NVR counters and the in-memory log.

pub mod logs;
mod metrics;
mod routes;

pub use metrics::Sampler;
pub use routes::router;
