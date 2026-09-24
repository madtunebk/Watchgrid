use super::{ApiResult, CapacityEstimate, LogEntry, LogQuery, ServerInfo, SystemStatus, backend};

/// GET /api/v1/server
pub async fn get_server_info() -> ApiResult<ServerInfo> {
    backend::system::server_info().await
}

/// GET /api/v1/system/status
pub async fn get_system_status() -> ApiResult<SystemStatus> {
    backend::system::status().await
}

/// GET /api/v1/system/capacity — projected load and how many cameras fit.
pub async fn get_capacity() -> ApiResult<CapacityEstimate> {
    backend::capacity::estimate().await
}

/// GET /api/v1/system/logs — newest last.
pub async fn get_logs(query: LogQuery) -> ApiResult<Vec<LogEntry>> {
    backend::logs::list(&query).await
}
