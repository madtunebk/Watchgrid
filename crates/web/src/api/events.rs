use super::{ApiResult, EventBulkRequest, EventBulkSummary, EventDetail, EventPage, EventQuery, Id, backend};

/// GET /api/v1/events?…  — newest first, paginated by `limit` / `offset`.
pub async fn get_events(query: EventQuery) -> ApiResult<EventPage> {
    backend::events::list(&query).await
}

/// GET /api/v1/events/{id}
pub async fn get_event(id: Id) -> ApiResult<EventDetail> {
    backend::events::get(&id).await
}

/// PUT /api/v1/events/{id}/protected — protected events are never removed by retention.
pub async fn set_event_protected(id: Id, protected: bool) -> ApiResult<()> {
    backend::events::set_protected(&id, protected).await
}

/// DELETE /api/v1/events/{id} — removes the event from the history; its
/// recording is kept. Refused while protected.
pub async fn delete_event(id: Id) -> ApiResult<()> {
    backend::events::delete(&id).await
}

/// POST /api/v1/events/bulk/preview — what a bulk action would do; changes nothing.
pub async fn preview_event_bulk(req: EventBulkRequest) -> ApiResult<EventBulkSummary> {
    backend::events::bulk_preview(&req).await
}

/// POST /api/v1/events/bulk — protect, unprotect or delete many events.
pub async fn apply_event_bulk(req: EventBulkRequest) -> ApiResult<EventBulkSummary> {
    backend::events::bulk_apply(&req).await
}
