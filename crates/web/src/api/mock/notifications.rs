//! Mock notification feed.

use gloo_timers::future::TimeoutFuture;

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiResult, Id, Notification};

pub async fn list() -> ApiResult<Vec<Notification>> {
    latency().await;
    let mut list = with_db(|db| db.notifications.clone());
    list.sort_by(|a, b| b.time.cmp(&a.time));
    Ok(list)
}

pub async fn mark_read(ids: Option<Vec<Id>>) -> ApiResult<()> {
    TimeoutFuture::new(60).await;
    with_db(|db| {
        for n in &mut db.notifications {
            if ids.as_ref().is_none_or(|ids| ids.contains(&n.id)) {
                n.read = true;
            }
        }
    });
    Ok(())
}
