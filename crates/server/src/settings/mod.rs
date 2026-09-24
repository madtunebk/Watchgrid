//! Server settings: the generic JSON store plus the Settings page document
//! (defaults, validation, HTTP API).

mod app;
mod routes;
mod store;

pub use app::load as load_app;
pub use routes::router;
pub use store::{load, save};

#[cfg(test)]
mod tests;
