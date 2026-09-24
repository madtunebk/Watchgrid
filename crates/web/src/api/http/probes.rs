//! Camera form tests against the real server.

use super::client;
use crate::api::{ApiResult, ConnectionProbe, ConnectionTest, OnvifConfig, OnvifProbe, StreamProbe, StreamTest};

pub async fn connection(req: ConnectionTest) -> ApiResult<ConnectionProbe> {
    client::post("/cameras/test-connection", Some(&req)).await
}

pub async fn stream(req: StreamTest) -> ApiResult<StreamProbe> {
    client::post("/cameras/test-stream", Some(&req)).await
}

/// ONVIF arrives with its own milestone; say so instead of pretending.
pub async fn onvif(_config: &OnvifConfig) -> ApiResult<OnvifProbe> {
    Ok(OnvifProbe {
        ok: false,
        message: "ONVIF testing arrives with ONVIF support in a later milestone".into(),
        event_topics: vec![],
        detections: vec![],
    })
}
