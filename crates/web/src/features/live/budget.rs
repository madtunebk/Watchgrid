//! How heavy the current page is for the viewer's browser: every tile is a
//! video the browser must decode.

use crate::api::{Camera, Stream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Light,
    Moderate,
    Heavy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageLoad {
    pub streams: usize,
    /// kbit/s pulled by this page.
    pub bitrate: u32,
    /// Decode work in "1080p @ 25 fps" equivalents.
    pub decode: f32,
    pub level: Level,
    pub using_substreams: bool,
}

/// Grids of 3×3 and larger show small tiles; the substream (when the
/// camera has one) looks the same there at a fraction of the cost.
pub fn prefers_substream(columns: usize) -> bool {
    columns >= 3
}

/// The stream a tile actually plays.
pub fn tile_stream(camera: &Camera, columns: usize) -> &Stream {
    match (&camera.sub_stream, prefers_substream(columns)) {
        (Some(sub), true) => sub,
        _ => &camera.main_stream,
    }
}

pub fn page_load(cameras: &[Camera], columns: usize) -> PageLoad {
    let live: Vec<&Camera> = cameras.iter().filter(|c| c.streaming()).collect();
    let mut bitrate = 0;
    let mut decode = 0.0;
    for cam in &live {
        let s = tile_stream(cam, columns);
        bitrate += s.bitrate.unwrap_or(4096);
        let pixels = s.width.unwrap_or(1920) as f32 * s.height.unwrap_or(1080) as f32;
        decode += pixels * s.fps.unwrap_or(25.0) / (1920.0 * 1080.0 * 25.0);
    }
    let level = match decode {
        d if d < 6.0 => Level::Light,
        d if d < 12.0 => Level::Moderate,
        _ => Level::Heavy,
    };
    PageLoad {
        streams: live.len(),
        bitrate,
        decode,
        level,
        using_substreams: prefers_substream(columns) && live.iter().any(|c| c.sub_stream.is_some()),
    }
}
