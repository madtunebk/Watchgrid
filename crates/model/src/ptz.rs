//! Pan / tilt / zoom for cameras that move (ONVIF PTZ).

use serde::{Deserialize, Serialize};

/// What the camera offers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzState {
    pub available: bool,
    pub presets: Vec<PtzPreset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzPreset {
    pub token: String,
    pub name: String,
}

/// Speeds from -1 to 1: pan right, tilt up and zoom in are positive. The
/// camera keeps moving about a second; repeat while the button is held.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzMove {
    pub pan: f32,
    pub tilt: f32,
    #[serde(default)]
    pub zoom: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzPresetInput {
    pub name: String,
}
