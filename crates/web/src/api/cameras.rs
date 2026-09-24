use super::{ApiResult, Camera, CameraInput, ConnectionProbe, ConnectionTest, OnvifConfig, OnvifProbe, StreamProbe, StreamTest, backend};

// ------------------------------------------------------------------ queries

/// GET /api/v1/cameras
pub async fn get_cameras() -> ApiResult<Vec<Camera>> {
    backend::cameras::list().await
}

/// GET /api/v1/cameras/{id}
pub async fn get_camera(id: String) -> ApiResult<Camera> {
    backend::cameras::get(&id).await
}

// ------------------------------------------------------------------ management

/// POST /api/v1/cameras — takes effect immediately, no restart.
pub async fn create_camera(input: CameraInput) -> ApiResult<Camera> {
    backend::cameras::create(input).await
}

/// PUT /api/v1/cameras/{id} — applied live by the camera's task.
pub async fn update_camera(id: String, input: CameraInput) -> ApiResult<Camera> {
    backend::cameras::update(&id, input).await
}

/// DELETE /api/v1/cameras/{id} — recordings are kept.
pub async fn delete_camera(id: String) -> ApiResult<()> {
    backend::cameras::delete(&id).await
}

/// POST /api/v1/cameras/{id}/enable | /disable
pub async fn set_camera_enabled(id: String, enabled: bool) -> ApiResult<Camera> {
    backend::cameras::set_enabled(&id, enabled).await
}

// ------------------------------------------------------------------ recording

/// POST /api/v1/cameras/{id}/recording/start
pub async fn start_recording(camera_id: String) -> ApiResult<Camera> {
    backend::cameras::set_manual_recording(&camera_id, true).await
}

/// POST /api/v1/cameras/{id}/recording/stop
pub async fn stop_recording(camera_id: String) -> ApiResult<Camera> {
    backend::cameras::set_manual_recording(&camera_id, false).await
}

// ------------------------------------------------------------------ tests before saving

/// POST /api/v1/cameras/test-connection
pub async fn test_connection(req: ConnectionTest) -> ApiResult<ConnectionProbe> {
    backend::probes::connection(req).await
}

/// POST /api/v1/cameras/test-stream — with `camera_id` and no password,
/// the server uses the stored one.
pub async fn test_stream(req: StreamTest) -> ApiResult<StreamProbe> {
    backend::probes::stream(req).await
}

/// POST /api/v1/cameras/test-onvif
pub async fn test_onvif(config: OnvifConfig) -> ApiResult<OnvifProbe> {
    backend::probes::onvif(&config).await
}
