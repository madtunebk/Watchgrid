//! In-memory state of the mock backend.

use std::cell::RefCell;

use super::scenario::{self, Scenario};
use super::seed;
use crate::api::{Camera, Event, ExportJob, ExportTarget, LogEntry, Notification, Recording, RetentionPolicy, ServerInfo, Session, Settings, User};

pub struct Db {
    pub scenario: Scenario,
    pub server: ServerInfo,
    pub cameras: Vec<Camera>,
    pub events: Vec<Event>,
    pub recordings: Vec<Recording>,
    pub notifications: Vec<Notification>,
    pub retention: RetentionPolicy,
    pub export_targets: Vec<ExportTarget>,
    pub export_jobs: Vec<ExportJob>,
    pub settings: Settings,
    pub users: Vec<User>,
    pub sessions: Vec<Session>,
    pub logs: Vec<LogEntry>,
}

thread_local! {
    static DB: RefCell<Db> = RefCell::new(seed::build(scenario::current()));
}

pub fn with_db<R>(f: impl FnOnce(&mut Db) -> R) -> R {
    DB.with(|db| f(&mut db.borrow_mut()))
}
