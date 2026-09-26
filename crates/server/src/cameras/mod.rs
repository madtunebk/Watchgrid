//! Camera configuration: validation, storage and the HTTP API.

mod probes;
pub mod ptz;
mod repo;
mod routes;
mod service;
mod url_credentials;
mod validate;

pub use routes::router;
pub use service::{stored as repo_get, all_ids, connection_info, onvif_watch, get as get_camera, list_live, stored_onvif_login, stream_credentials};

#[cfg(test)]
mod tests;
