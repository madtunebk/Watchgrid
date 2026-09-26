//! Removing a recording, for the API and for retention alike.
//!
//! One transaction, holding the clip's lock (see [`lock_clip`]): check the
//! protection (by hand or by any linked event) and pending uploads (an
//! export still needs the file), unlink the events, delete
//! the row, remove the file, commit. A failed file removal rolls everything
//! back; a missing file counts as already deleted.

use std::fmt;

use sqlx::{PgConnection, PgPool};

use super::RecordingFiles;
use super::repo::EFFECTIVELY_PROTECTED;

/// SQL: an upload of recording `r` still needs its file.
pub const EXPORT_PENDING: &str = "EXISTS (SELECT 1 FROM export_jobs j WHERE j.recording_id = r.id AND j.state IN ('queued', 'uploading'))";

/// Whether a saved clip exists (call under its lock).
pub async fn clip_exists(tx: &mut PgConnection, recording_id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM recordings WHERE id = $1)").bind(recording_id).fetch_one(tx).await
}

/// Advisory-lock namespace for recordings (the two-key form never collides
/// with the single-key server lock).
const LOCK_SPACE: i32 = 0x5747_5243; // "WGRC"

/// Serialize everything that changes a clip's protection or removes it.
/// Held until the transaction ends. Works for clips still recording (not
/// yet in `recordings`), so protecting an event mid-recording is covered.
pub async fn lock_clip(tx: &mut PgConnection, recording_id: &str) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))").bind(LOCK_SPACE).bind(recording_id).execute(tx).await.map(|_| ())
}

#[derive(Debug, PartialEq)]
pub enum DeleteError {
    /// Protected by hand or by at least one of its events.
    Protected,
    /// An upload of it is queued, running or waiting for a retry.
    Exporting,
    Failed(String),
}

impl fmt::Display for DeleteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protected => f.write_str("the recording is protected"),
            Self::Exporting => f.write_str("an upload of the recording is still pending"),
            Self::Failed(e) => f.write_str(e),
        }
    }
}

impl From<sqlx::Error> for DeleteError {
    fn from(e: sqlx::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

/// Delete a finished recording and its file. `Ok(false)`: it was already gone.
pub async fn delete_recording(db: &PgPool, files: &RecordingFiles, id: &str) -> Result<bool, DeleteError> {
    let mut tx = db.begin().await?;
    lock_clip(&mut tx, id).await?;
    let row: Option<(Option<String>, String, bool)> =
        sqlx::query_as(&format!("SELECT r.root, r.path, {EFFECTIVELY_PROTECTED} FROM recordings r WHERE r.id = $1 FOR UPDATE"))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((root, relative, protected)) = row else { return Ok(false) };
    if protected {
        return Err(DeleteError::Protected);
    }
    // Enqueuing takes the same lock, so no job can slip in after this.
    let exporting: bool = sqlx::query_scalar(&format!("SELECT {EXPORT_PENDING} FROM (SELECT $1::text AS id) r")).bind(id).fetch_one(&mut *tx).await?;
    if exporting {
        return Err(DeleteError::Exporting);
    }
    let path = files.resolve(root.as_deref(), &relative).ok_or_else(|| DeleteError::Failed("unsafe recording path".into()))?;
    // Events stay in the history, without video.
    sqlx::query("UPDATE events SET recording_id = NULL WHERE recording_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM recordings WHERE id = $1").bind(id).execute(&mut *tx).await?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(DeleteError::Failed(format!("{}: {e}", path.display()))),
    }
    tx.commit().await?;
    Ok(true)
}
