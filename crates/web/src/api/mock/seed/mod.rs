//! Seed data for the mock backend, per scenario.

mod cameras;
mod exports;
mod logs;
mod people;
mod settings;
mod history;
mod notifications;
mod server;

use super::db::Db;
use super::scenario::Scenario;
use crate::api::RetentionPolicy;

const GB: u64 = 1_000_000_000;

pub fn build(scenario: Scenario) -> Db {
    let mut server = server::info();
    if scenario == Scenario::Empty {
        server.name = "Watchgrid".into(); // a fresh install keeps the default name
    }
    let (settings, users, sessions) = (settings::defaults(&server.name), people::users(), people::sessions());
    let retention = RetentionPolicy { max_age_days: Some(14), max_usage: Some(500 * GB), min_free: Some(50 * GB) };

    if scenario == Scenario::Empty {
        return Db { scenario, server, cameras: vec![], events: vec![], recordings: vec![], notifications: vec![], retention, export_targets: vec![], export_jobs: vec![], settings, users, sessions, logs: logs::boot() };
    }

    let mut cameras = if scenario == Scenario::Large { cameras::many(40) } else { cameras::all() };
    let (events, recordings) = history::generate(&cameras);
    history::attach_last_events(&mut cameras, &events);
    let logs = logs::history(&cameras);

    Db { scenario, server, cameras, events, recordings, notifications: notifications::all(), retention, export_targets: exports::targets(), export_jobs: vec![], settings, users, sessions, logs }
}
