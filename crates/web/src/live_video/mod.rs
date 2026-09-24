//! Live camera video in the browser (feature `live-api`).
//!
//! The server streams fragmented MP4 over a WebSocket
//! (`/api/v1/cameras/{id}/live?stream=main|sub`); a [`player::Player`]
//! feeds it into Media Source Extensions. Players are pooled so the many
//! re-renders of camera widgets don't reconnect the stream.

mod player;
mod pool;
mod view;

pub use view::LiveVideo;
