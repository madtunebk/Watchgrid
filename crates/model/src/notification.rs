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
}
