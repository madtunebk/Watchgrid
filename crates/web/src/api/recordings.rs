use super::{ApiResult, Id, Recording, RecordingQuery, backend};

/// GET /api/v1/recordings?… — clips overlapping the range, oldest first.
/// URL of a recording's video, when the backend stores real files.
pub fn recording_media_url(id: &str) -> Option<String> {
    backend::recordings::media_url(id)
}

pub async fn get_recordings(query: RecordingQuery) -> ApiResult<Vec<Recording>> {
    backend::recordings::list(&query).await
}

/// PUT /api/v1/recordings/{id}/protected — keep a clip from retention.
pub async fn set_recording_protected(id: Id, protected: bool) -> ApiResult<()> {
    backend::recordings::set_protected(&id, protected).await
}

/// DELETE /api/v1/recordings/{id} — removes the video; its events stay in
/// the history. Refused while recording or protected.
pub async fn delete_recording(id: Id) -> ApiResult<()> {
    backend::recordings::delete(&id).await
}
