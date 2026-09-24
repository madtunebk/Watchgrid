//! NVR features. Each module owns its pages and domain widgets and talks to
//! the backend only through `crate::api`.

pub mod cameras;
pub mod dashboard;
pub mod events;
pub mod live;
pub mod playback;
pub mod recordings;
pub mod settings;
pub mod storage;
pub mod system;
