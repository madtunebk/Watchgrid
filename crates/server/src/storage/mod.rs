//! Storage: the recordings volume, per-camera usage and the retention
//! policy, which the sweeper enforces.

mod disk;
pub mod plan;
mod retention;
mod routes;
mod sweeper;

pub use routes::router;
pub use sweeper::Sweeper;

#[cfg(test)]
mod tests;
