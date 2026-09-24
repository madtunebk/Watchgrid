//! Storage: the recordings volume, per-camera usage and the retention
//! policy, which the sweeper enforces.

mod disk;
pub mod plan;
mod retention;
mod routes;
mod sweeper;

pub use routes::router;
pub use sweeper::Sweeper;

/// Percent of the recordings volume in use, if readable.
pub fn disk_usage_percent(path: &std::path::Path) -> Option<f32> {
    let d = disk::space(path).ok()?;
    (d.total > 0).then(|| d.used as f32 / d.total as f32 * 100.0)
}

#[cfg(test)]
mod tests;
