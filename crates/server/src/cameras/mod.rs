//! Camera configuration: validation, storage and the HTTP API.

mod probes;
mod repo;
mod routes;
mod service;
mod url_credentials;
mod validate;

pub use routes::router;
pub use service::{all_ids, connection_info, get as get_camera, stream_credentials};

#[cfg(test)]
mod tests;
