//! Durable events: camera outages, recordings (and later motion, ONVIF and
//! vendor integrations), stored from bus transitions by the journal.

mod journal;
mod kinds;
mod repo;
mod routes;

pub use journal::start as start_journal;
pub use routes::router;

#[cfg(test)]
mod tests;
