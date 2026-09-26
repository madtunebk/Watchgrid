//! Results of connection / stream / ONVIF tests run before saving a camera.

use serde::{Deserialize, Serialize};

use crate::EventType;

/// Result of `POST /cameras/test-connection` (host reachable, credentials ok).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProbe {
    pub ok: bool,
    pub message: String,
    pub latency_ms: Option<u32>,
    /// Manufacturer / model when the device reports it.
    pub device: Option<String>,
}

/// Result of `POST /cameras/test-onvif`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnvifProbe {
    pub ok: bool,
    pub message: String,
    /// Event topics the camera advertises, e.g. "RuleEngine/CellMotionDetector/Motion".
    pub event_topics: Vec<String>,
    /// Detection kinds the NVR can map from those topics.
    pub detections: Vec<EventType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamProbe {
    pub ok: bool,
    pub message: String,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    pub audio_codec: Option<String>,
    pub latency_ms: Option<u32>,
}

/// Body of `POST /cameras/test-connection`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub host: String,
    pub username: String,
    /// Empty/absent with `camera_id` set = use the stored password.
    pub password: Option<String>,
    /// Set when testing an existing camera from its edit form.
    pub camera_id: Option<String>,
}

/// Body of `POST /cameras/test-stream`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamTest {
    pub url: String,
    pub username: String,
    /// Empty/absent with `camera_id` set = use the stored password.
    pub password: Option<String>,
    pub camera_id: Option<String>,
}
