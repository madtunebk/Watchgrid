//! Recordings: finalized video files on disk plus their metadata in
//! PostgreSQL, and the HTTP API to list and play them.

mod files;
mod repo;
mod routes;

pub use files::RecordingFiles;
pub use repo::{NewRecording, delete, insert, protected_bytes, retention_candidates, usage_by_camera};
pub use routes::router;

#[cfg(test)]
mod tests;
