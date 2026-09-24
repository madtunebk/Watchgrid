//! ONVIF (SOAP over HTTP) — just what Watchgrid needs, hand-written:
//! device information, the event service address and its topics. Event
//! subscriptions (PullPoint) build on this.

mod client;
mod http;
mod probe;
mod topics;
mod xml;

pub use probe::probe;
