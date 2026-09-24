//! ONVIF (SOAP over HTTP) — just what Watchgrid needs, hand-written:
//! device information, the event service address and its topics. Event
//! subscriptions (PullPoint) feed motion/person events via the watchers.

mod client;
mod http;
mod probe;
pub mod pullpoint;
pub mod topics;
mod watcher;
mod xml;

pub use probe::probe;
pub use watcher::{Deps as WatchDeps, Watchers};
pub use xml::Notification;
