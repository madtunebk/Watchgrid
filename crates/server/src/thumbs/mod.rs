//! Event thumbnails: a small JPEG of the event's moment in its recording,
//! made once on first request and kept in `<recordings>/.thumbs/`. The
//! browser loads a ~15 KB picture instead of opening the video.

mod picture;

use std::path::PathBuf;
use std::sync::LazyLock;

use sqlx::PgPool;
use tokio::sync::Semaphore;

use crate::media::mp4_read;
use crate::recordings::RecordingFiles;

/// Thumbnail width in pixels (rows show it at ~60 px, retina included).
const WIDTH: usize = 320;
/// Seconds after the event start (the moment the event list points at).
const INTO_EVENT: f64 = 0.5;
/// At most this many thumbnails are made at once (each decodes one frame).
static MAKING: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));

/// Folder of the cached thumbnails.
pub fn folder(files: &RecordingFiles) -> PathBuf {
    files.root().join(".thumbs")
}

fn cached(files: &RecordingFiles, event_id: &str) -> Option<PathBuf> {
    // Ids are generated ("evt-123"); anything else never names a file.
    let safe = !event_id.is_empty() && event_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| folder(files).join(format!("{event_id}.jpg")))
}

/// The event's thumbnail, made if needed. `Ok(None)`: the event has no
/// saved recording (yet).
pub async fn for_event(db: &PgPool, files: &RecordingFiles, event_id: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = cached(files, event_id) else { return Ok(None) };
    if let Ok(jpeg) = tokio::fs::read(&path).await {
        return Ok(Some(jpeg));
    }
    let Some((start, rec_start, recording_id)) = moment(db, event_id).await.map_err(|e| e.to_string())? else { return Ok(None) };
    let Some(file) = crate::recordings::file_of(db, files, &recording_id).await.map_err(|e| e.to_string())? else { return Ok(None) };

    let _turn = MAKING.acquire().await.map_err(|e| e.to_string())?;
    let index = mp4_read::read_index(&file).await?;
    let at = (start - rec_start).num_milliseconds() as f64 / 1000.0 + INTO_EVENT;
    let sample = index.keyframe_at(at).ok_or("the recording has no keyframe")?;
    let frame = mp4_read::read_sample(&file, sample).await?;
    let avcc = index.avcc;
    let jpeg = tokio::task::spawn_blocking(move || picture::jpeg(&avcc, &frame, WIDTH)).await.map_err(|e| e.to_string())??;

    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    if let Err(e) = tokio::fs::write(&path, &jpeg).await {
        tracing::debug!("cannot cache the thumbnail of {event_id}: {e}");
    }
    Ok(Some(jpeg))
}

/// (event start, recording start, recording id) for an event with a saved clip.
async fn moment(db: &PgPool, event_id: &str) -> sqlx::Result<Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, String)>> {
    sqlx::query_as("SELECT e.start_time, r.start_time, r.id FROM events e JOIN recordings r ON r.id = e.recording_id WHERE e.id = $1")
        .bind(event_id)
        .fetch_optional(db)
        .await
}

/// Remove thumbnails of events that are gone or lost their recording.
/// Returns how many were removed.
pub async fn prune(db: &PgPool, files: &RecordingFiles) -> Result<usize, String> {
    let dir = folder(files);
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else { return Ok(0) };
    let mut names = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        if let Some(id) = entry.file_name().to_str().and_then(|n| n.strip_suffix(".jpg")) {
            names.push(id.to_string());
        }
    }
    let mut removed = 0;
    for batch in names.chunks(500) {
        let keep: Vec<String> = sqlx::query_scalar("SELECT id FROM events WHERE id = ANY($1) AND recording_id IS NOT NULL")
            .bind(batch)
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
        for id in batch.iter().filter(|id| !keep.contains(id)) {
            if tokio::fs::remove_file(dir.join(format!("{id}.jpg"))).await.is_ok() {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    /// Manual: `WATCHGRID_THUMB_TEST=<recording.mp4> cargo test -p watchgrid-server real_recording -- --ignored`
    /// writes `<recording>.thumb.jpg` next to it.
    #[tokio::test]
    #[ignore]
    async fn real_recording_becomes_a_thumbnail() {
        let path = std::path::PathBuf::from(std::env::var("WATCHGRID_THUMB_TEST").expect("set WATCHGRID_THUMB_TEST"));
        let index = crate::media::mp4_read::read_index(&path).await.unwrap();
        let sample = index.keyframe_at(5.0).unwrap();
        let frame = crate::media::mp4_read::read_sample(&path, sample).await.unwrap();
        let started = std::time::Instant::now();
        let jpeg = super::picture::jpeg(&index.avcc, &frame, super::WIDTH).unwrap();
        println!("{} samples, keyframe {} bytes → JPEG {} bytes in {:?}", index.samples.len(), frame.len(), jpeg.len(), started.elapsed());
        std::fs::write(path.with_extension("thumb.jpg"), jpeg).unwrap();
    }
}
