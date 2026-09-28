//! Armed surveillance over HTTP — same functions as the mock's `arm` module.

use super::client;
use crate::api::{ApiResult, ArmInput, ArmState};

pub async fn state() -> ApiResult<ArmState> {
    client::get("/arm").await
}

pub async fn change(input: ArmInput) -> ApiResult<ArmState> {
    client::post("/arm", Some(&input)).await
}
