//! The UI's only door to the NVR.
//!
//! Every function here mirrors a future `/api/v1` endpoint and returns the
//! shared `watchgrid_model` types. Which implementation serves each domain
//! is decided in one place, `backend.rs`: the in-memory mock, or the real
//! server over HTTP (feature `live-api`) for domains already migrated.
//! Components call these functions through [`query`] hooks and never import
//! the mock directly.

mod auth;
mod backend;
mod cameras;
mod connection;
mod error;
mod events;
mod exports;
#[cfg(feature = "live-api")]
mod http;
mod mock;
mod notifications;
#[cfg(feature = "live-api")]
mod push;
pub mod query;
mod recordings;
mod settings;
mod storage;
mod system;

pub use auth::*;
pub use cameras::*;
pub use watchgrid_model::*;
pub use connection::{ConnectionState, provide_connection, use_connection};
pub use error::{ApiError, ApiResult};
pub use events::*;
pub use exports::*;
pub use notifications::*;
pub use query::{Topic, invalidate, provide_queries, use_query};
pub use recordings::*;
pub use settings::*;
pub use storage::*;
pub use system::*;

