//! Users and sessions. The UI can show and manage them; enforcing
//! authentication is a backend milestone.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    /// Live view and playback only.
    Viewer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Id,
    pub username: String,
    pub role: Role,
    pub last_login: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: Id,
    pub username: String,
    /// "Chrome on Linux", "Firefox on Android"…
    pub client: String,
    pub address: String,
    pub last_seen: Timestamp,
    /// The session making this request.
    pub current: bool,
}
