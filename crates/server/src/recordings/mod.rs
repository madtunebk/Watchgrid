//! Recordings: finalized video files on disk plus their metadata in
//! PostgreSQL, and the HTTP API to list and play them.

mod delete;
mod files;
mod repo;
mod routes;

pub use files::RecordingFiles;
pub use delete::delete_recording;
pub use repo::{NewRecording, get, insert, protected_bytes, retention_candidates, set_protected, usage_by_camera};
pub use routes::router;

#[cfg(test)]
mod tests;
