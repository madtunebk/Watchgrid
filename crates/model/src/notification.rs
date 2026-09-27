//! User-facing notifications.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: Id,
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub time: Timestamp,
    pub read: bool,
    /// In-app route to open when clicked.
    pub link: Option<String>,
    /// The camera it is about, if any.
    #[serde(default)]
    pub camera_id: Option<Id>,
}

/// Which notifications a page shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationFilter {
    pub unread_only: bool,
    pub camera_id: Option<Id>,
    /// Warnings and errors only.
    pub problems_only: bool,
}

/// One page of notifications, newest first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPage {
    pub items: Vec<Notification>,
    /// Notifications matching the filter (all pages).
    pub total: u32,
    /// Unread notifications overall, whatever the filter.
    pub unread: u32,
}

/// Most notifications one bulk request may change.
pub const NOTIFICATION_BULK_MAX: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationBulkAction {
    Read,
    Unread,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationBulkRequest {
    pub ids: Vec<Id>,
    pub action: NotificationBulkAction,
}

/// How many notifications a bulk action changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationBulkResult {
    pub changed: u32,
}

/// What happened to a test notification.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationResult {
    /// The webhook's outcome ("HTTP 200", or the error); `None` without a webhook.
    pub webhook: Option<String>,
    /// Whether the webhook accepted it.
    pub webhook_ok: bool,
}
