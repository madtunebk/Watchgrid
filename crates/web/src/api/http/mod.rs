//! HTTP implementation of the API against the Watchgrid server (`/api/v1`).

pub mod auth;
pub mod cameras;
pub mod events;
pub mod logs;
pub mod notifications;
mod client;
pub mod probes;
pub mod recordings;
pub mod settings;
pub mod storage;
pub mod system;
