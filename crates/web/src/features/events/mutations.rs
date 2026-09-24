//! Event write operations: call the API, then refresh what they affect.

use crate::api::{self, ApiResult, Topic, invalidate};

fn refresh() {
    invalidate(Topic::Events);
    invalidate(Topic::Recordings);
    invalidate(Topic::Storage);
}

pub async fn set_protected(id: String, protected: bool) -> ApiResult<()> {
    api::set_event_protected(id, protected).await?;
    refresh();
    Ok(())
}

pub async fn delete(id: String) -> ApiResult<()> {
    api::delete_event(id).await?;
    refresh();
    invalidate(Topic::Cameras); // "last event" may change
    Ok(())
}
