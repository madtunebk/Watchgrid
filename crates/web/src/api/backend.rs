//! The one place that decides who serves each API domain.
//!
//! With the `live-api` feature, domains that exist on the Watchgrid server
//! go over HTTP; everything else stays on the in-browser mock until its
//! milestone. Without the feature the whole UI runs on the mock.
//! Real so far: cameras, probes, recordings, storage, events, system, logs, settings, auth, notifications, capacity.

#[cfg(feature = "live-api")]
pub use super::http::{auth, capacity, cameras, events, logs, notifications, probes, recordings, settings, storage, system};
#[cfg(not(feature = "live-api"))]
pub use super::mock::{auth, capacity, cameras, events, logs, notifications, probes, recordings, settings, storage, system};

pub use super::mock::exports;
