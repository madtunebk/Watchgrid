//! Playback: the clip player used by events and recordings. The real video
//! element (MSE / HLS) will live here once the recording engine exists.

mod clip;
mod player;

pub use clip::Clip;
pub use player::Player;
