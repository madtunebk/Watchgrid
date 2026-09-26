//! Durable events: camera outages, recordings (and later motion, ONVIF and
//! vendor integrations), stored from bus transitions by the journal.

mod bulk;
mod journal;
mod kinds;
mod repo;
mod routes;

pub use bulk::ClipRow;
pub use journal::start as start_journal;
pub use repo::{delete_many as delete_events, get as get_event, of_recording as events_of_recording, purge_older_than as purge_events_older_than};
pub use routes::router;

#[cfg(test)]
mod tests;
