//! "How many cameras can this machine take?" — from measured hardware,
//! Watchgrid's own measured cost and the cameras' real bitrates and
//! recording duty over the last day.

mod estimate;
mod hardware;
mod routes;

pub use routes::estimate as handler;
