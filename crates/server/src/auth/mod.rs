//! Authentication and authorization.
//!
//! Users exist only through the CLI (`watchgrid user …`); the browser can
//! sign in and out, list sessions and revoke them — never create, delete
//! or reset users. Every API request except health/login/session needs a
//! valid session; viewers are read-only.

mod client_ip;
pub mod cli;
mod guard;
mod limiter;
mod password;
mod prompt;
mod routes;
mod sessions;
mod tokens;
mod users;

pub use guard::{CurrentUser, require_session};
pub use limiter::LoginLimiter;
pub use routes::router;

/// Name of the session cookie.
pub const COOKIE: &str = "wg_session";

#[cfg(test)]
mod tests;
