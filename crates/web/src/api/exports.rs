use super::{ApiResult, AutoUpload, ConnectionProbe, ExportJob, ExportTarget, ExportTargetInput, Id, backend};

/// GET /api/v1/exports/targets — configured destinations (no secrets).
pub async fn get_export_targets() -> ApiResult<Vec<ExportTarget>> {
    backend::exports::targets().await
}

/// POST /api/v1/events/{id}/export — queue an upload of the event's clip.
pub async fn export_event(event_id: Id, target_id: Id) -> ApiResult<ExportJob> {
    backend::exports::start(&event_id, &target_id).await
}

/// POST /api/v1/recordings/{id}/export — queue an upload of a recording.
pub async fn export_recording(recording_id: Id, target_id: Id) -> ApiResult<ExportJob> {
    backend::exports::start_recording(&recording_id, &target_id).await
}

/// GET /api/v1/exports/jobs/{id} — poll progress.
pub async fn get_export_job(id: Id) -> ApiResult<ExportJob> {
    backend::exports::job(&id).await
}

/// POST /api/v1/exports/targets/test — check credentials before saving.
pub async fn test_export_target(input: ExportTargetInput) -> ApiResult<ConnectionProbe> {
    backend::exports::test(&input).await
}

/// POST /api/v1/exports/targets
pub async fn create_export_target(input: ExportTargetInput) -> ApiResult<ExportTarget> {
    backend::exports::create(input).await
}

/// PUT /api/v1/exports/targets/{id}/auto-upload
pub async fn set_export_auto_upload(id: Id, rule: AutoUpload) -> ApiResult<()> {
    backend::exports::set_auto(&id, rule).await
}

/// POST /api/v1/exports/targets/{id}/reconnect — redo sign-in (OAuth).
pub async fn reconnect_export_target(id: Id) -> ApiResult<()> {
    backend::exports::reconnect(&id).await
}

/// DELETE /api/v1/exports/targets/{id}
pub async fn delete_export_target(id: Id) -> ApiResult<()> {
    backend::exports::delete(&id).await
}
