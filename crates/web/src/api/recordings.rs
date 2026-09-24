use super::{ApiResult, Recording, RecordingQuery, backend};

/// GET /api/v1/recordings?… — clips overlapping the range, oldest first.
/// URL of a recording's video, when the backend stores real files.
pub fn recording_media_url(id: &str) -> Option<String> {
    backend::recordings::media_url(id)
}

pub async fn get_recordings(query: RecordingQuery) -> ApiResult<Vec<Recording>> {
    backend::recordings::list(&query).await
}
