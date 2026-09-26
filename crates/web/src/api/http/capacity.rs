//! Capacity estimate over HTTP — same function as the mock's `capacity` module.

use super::client;
use crate::api::{ApiResult, CapacityEstimate};

pub async fn estimate() -> ApiResult<CapacityEstimate> {
    client::get("/system/capacity").await
}
