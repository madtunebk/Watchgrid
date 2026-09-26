//! Mock notification feed.

use gloo_timers::future::TimeoutFuture;

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiResult, Id, Notification, NotificationBulkAction, NotificationBulkRequest, NotificationBulkResult, NotificationLevel, NotificationPage, TestNotificationResult};

pub async fn list(unread_only: bool, limit: u32, offset: u32) -> ApiResult<NotificationPage> {
    latency().await;
    Ok(with_db(|db| {
        let mut all = db.notifications.clone();
        all.sort_by_key(|n| std::cmp::Reverse(n.time));
        let unread = all.iter().filter(|n| !n.read).count() as u32;
        let matching: Vec<_> = all.into_iter().filter(|n| !unread_only || !n.read).collect();
        let total = matching.len() as u32;
        let items = matching.into_iter().skip(offset as usize).take(limit as usize).collect();
        NotificationPage { items, total, unread }
    }))
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

pub async fn bulk(req: &NotificationBulkRequest) -> ApiResult<NotificationBulkResult> {
    latency().await;
    Ok(with_db(|db| {
        let before = db.notifications.len();
        let mut changed = 0;
        match req.action {
            NotificationBulkAction::Delete => {
                db.notifications.retain(|n| !req.ids.contains(&n.id));
                changed = before - db.notifications.len();
            }
            NotificationBulkAction::Read | NotificationBulkAction::Unread => {
                let read = req.action == NotificationBulkAction::Read;
                for n in db.notifications.iter_mut().filter(|n| req.ids.contains(&n.id) && n.read != read) {
                    n.read = read;
                    changed += 1;
                }
            }
        }
        NotificationBulkResult { changed: changed as u32 }
    }))
}

pub async fn test() -> ApiResult<TestNotificationResult> {
    latency().await;
    Ok(with_db(|db| {
        let n = db.notifications.len();
        db.notifications.push(Notification {
            id: format!("test-{n}"),
            level: NotificationLevel::Info,
            title: "Test notification".into(),
            message: "Sent from Settings → Notifications. If you see this, notifications work.".into(),
            time: chrono::Utc::now(),
            read: false,
            link: Some("/notifications".into()),
        });
        let webhook = db.settings.notifications.webhook_url.as_ref().map(|_| "HTTP 200 (demo)".to_string());
        TestNotificationResult { webhook_ok: webhook.is_some(), webhook }
    }))
}
