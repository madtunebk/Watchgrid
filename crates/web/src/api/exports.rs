use super::{ApiResult, AutoUpload, ConnectionProbe, ExportJob, ExportTarget, ExportTargetInput, ExportTargetSettings, Id, backend};

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

/// GET /api/v1/exports/jobs — uploads not finished yet (queued, waiting
/// for a retry, running), oldest first.
pub async fn get_pending_exports() -> ApiResult<Vec<ExportJob>> {
    backend::exports::pending().await
}

/// POST /api/v1/exports/jobs/{id}/cancel — stop an upload that hasn't finished.
pub async fn cancel_export(id: Id) -> ApiResult<ExportJob> {
    backend::exports::cancel(&id).await
}

/// POST /api/v1/exports/targets/test — check credentials before saving.
pub async fn test_export_target(input: ExportTargetInput) -> ApiResult<ConnectionProbe> {
    backend::exports::test(&input).await
}

/// POST /api/v1/exports/targets
pub async fn create_export_target(input: ExportTargetInput) -> ApiResult<ExportTarget> {
    backend::exports::create(input).await
}

/// GET /api/v1/exports/targets/{id} — saved settings for the edit form (no secret).
pub async fn get_export_target_settings(id: Id) -> ApiResult<ExportTargetSettings> {
    backend::exports::settings(&id).await
}

/// POST /api/v1/exports/targets/{id}/test — try an edit; no secret keeps the saved one.
pub async fn test_saved_export_target(id: Id, input: ExportTargetInput) -> ApiResult<ConnectionProbe> {
    backend::exports::test_saved(&id, &input).await
}

/// PUT /api/v1/exports/targets/{id} — save an edit (tested first).
pub async fn update_export_target(id: Id, input: ExportTargetInput) -> ApiResult<ExportTarget> {
    backend::exports::update(&id, input).await
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
