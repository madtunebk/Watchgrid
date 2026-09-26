//! Notifications: the bell in the UI and an optional webhook. Raised from
//! bus transitions according to Settings → Notifications, throttled so a
//! flapping camera doesn't flood anyone.

mod notifier;
mod repo;
mod routes;
mod rules;
mod webhook;

pub use notifier::start;
pub use routes::router;

#[cfg(test)]
mod tests;
