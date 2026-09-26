//! In-memory mock backend. Simulates latency and live values so every UI
//! state (loading, updating, empty) can be exercised without a server.
//! State resets on page reload.
//!
//! Scenarios: append `?mock=empty` (fresh install, no cameras),
//! `?mock=nostorage` (storage unavailable) or `?mock=large` (40 cameras)
//! to the URL, then reload.

pub mod auth;
pub mod capacity;
pub mod cameras;
pub mod events;
pub mod exports;
pub mod logs;
pub mod notifications;
pub mod probes;
pub mod recordings;
pub mod settings;
pub mod storage;
pub mod system;

mod db;
mod scenario;
mod seed;
mod sim;
