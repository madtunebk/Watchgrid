//! Recordings: finalized video files on disk plus their metadata in
//! PostgreSQL, and the HTTP API to list and play them.

mod delete;
mod files;
mod live_media;
mod repo;
mod routes;

pub use files::RecordingFiles;

/// Absolute path of a recording's file, if the recording exists.
pub async fn file_of(db: &sqlx::PgPool, files: &RecordingFiles, id: &str) -> sqlx::Result<Option<std::path::PathBuf>> {
    Ok(repo::path(db, id).await?.and_then(|(root, rel)| files.resolve(root.as_deref(), &rel)))
}
pub use delete::delete_recording;
pub use repo::{NewRecording, get, insert, protected_bytes, retention_candidates, set_protected, usage_by_camera};
pub use routes::{live as live_recording, router};

#[cfg(test)]
mod tests;
