//! Watchgrid data model.
//!
//! These types are the contract between the web UI and the backend. The UI
//! uses them today with an in-memory mock; the future Axum backend will
//! serialise the very same structs on `/api/v1`, so the two cannot drift.
//!
//! Conventions: JSON fields are camelCase, enum values snake_case,
//! timestamps UTC, sizes in bytes, durations in seconds, bitrates in kbit/s.

use chrono::{DateTime, Utc};

pub type Id = String;
pub type Timestamp = DateTime<Utc>;

mod auth;
mod camera;
mod capacity;
mod event;
mod export;
mod notification;
mod probe;
mod recording;
mod settings;
mod storage;
mod system;

pub use auth::*;
pub use camera::*;
pub use capacity::*;
pub use event::*;
pub use export::*;
pub use notification::*;
pub use probe::*;
pub use recording::*;
pub use settings::*;
pub use storage::*;
pub use system::*;
