//! Exporting clips to external storage (S3-compatible, Nextcloud/WebDAV;
//! Google Drive and Dropbox later). Secrets are sealed with the credential
//! store and never leave the server.

mod providers;
pub(crate) mod repo;
mod routes;
mod service;

pub use routes::{export_event, export_recording, router};
pub use service::Exports;

#[cfg(test)]
mod tests;
