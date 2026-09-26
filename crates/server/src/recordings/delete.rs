//! Removing a recording: the file first, then its row, so a listed
//! recording never points at nothing for long (a missing file counts as
//! already deleted). Protected recordings are never removed.

use sqlx::PgPool;

use super::{RecordingFiles, repo};

pub async fn delete_recording(db: &PgPool, files: &RecordingFiles, id: &str) -> Result<(), String> {
    let Some((root, relative, protected)) = repo::path_and_protection(db, id).await.map_err(|e| e.to_string())? else {
        return Ok(()); // already gone
    };
    if protected {
        return Err("the recording is protected".into());
    }
    let path = files.resolve(root.as_deref(), &relative).ok_or("unsafe recording path")?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    repo::delete(db, id).await.map_err(|e| e.to_string())
}
