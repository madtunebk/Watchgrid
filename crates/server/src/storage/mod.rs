//! Storage: the recordings volume, per-camera usage and the retention
//! policy, which the sweeper enforces.

mod disk;
pub mod plan;
mod retention;
mod routes;
mod sweeper;

pub use routes::router;
pub use sweeper::Sweeper;

/// (free bytes, "almost full" threshold) of the recordings volume: the
/// retention policy's minimum free space, and never less than 5 %.
pub async fn low_space(db: &sqlx::PgPool, path: &std::path::Path) -> Option<(u64, u64)> {
    let d = disk::space(path).ok()?;
    let min_free = retention::load(db).await.ok().and_then(|p| p.min_free).unwrap_or(0);
    Some((d.free, min_free.max(d.total / 20)))
}

/// Percent of the recordings volume in use, if readable.
pub fn disk_usage_percent(path: &std::path::Path) -> Option<f32> {
    let d = disk::space(path).ok()?;
    (d.total > 0).then(|| d.used as f32 / d.total as f32 * 100.0)
}

#[cfg(test)]
mod tests;
