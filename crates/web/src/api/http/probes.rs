//! Camera form tests against the real server.

use super::client;
use crate::api::{ApiResult, ConnectionProbe, ConnectionTest, OnvifConfig, OnvifProbe, StreamProbe, StreamTest};

pub async fn connection(req: ConnectionTest) -> ApiResult<ConnectionProbe> {
    client::post("/cameras/test-connection", Some(&req)).await
}

pub async fn stream(req: StreamTest) -> ApiResult<StreamProbe> {
    client::post("/cameras/test-stream", Some(&req)).await
}

/// Checks the ONVIF address and login, and lists the camera's event topics.
pub async fn onvif(config: &OnvifConfig) -> ApiResult<OnvifProbe> {
    client::post("/cameras/test-onvif", Some(config)).await
}
