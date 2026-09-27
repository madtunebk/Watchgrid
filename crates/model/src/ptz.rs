//! Pan / tilt / zoom for cameras that move (ONVIF PTZ).

use serde::{Deserialize, Serialize};

/// What the camera offers. Movement and presets are separate: a camera
/// that won't list its presets can still turn.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzState {
    pub available: bool,
    pub presets: Vec<PtzPreset>,
    /// Why the presets couldn't be listed (then `presets` is empty).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presets_error: Option<String>,
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
