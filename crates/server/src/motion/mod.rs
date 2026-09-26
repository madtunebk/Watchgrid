//! Software motion detection for cameras that can't report motion
//! themselves (no ONVIF events): the substream is decoded with OpenH264 and
//! compared against a learned background. Results go on the bus exactly
//! like camera-reported detections.

mod analyzer;
#[cfg(target_env = "musl")]
mod cxxrt;
pub(crate) mod decoder;
mod watcher;

pub use watcher::{Deps, Detectors, TOPIC};
