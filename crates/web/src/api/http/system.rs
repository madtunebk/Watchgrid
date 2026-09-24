//! System info and status over HTTP — same functions as the mock's `system` module.

use super::client;
use crate::api::{ApiResult, ServerInfo, SystemStatus};

pub async fn server_info() -> ApiResult<ServerInfo> {
    client::get("/system/info").await
}

pub async fn status() -> ApiResult<SystemStatus> {
    client::get("/system/status").await
}
