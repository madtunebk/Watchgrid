//! Events: things that happened on a camera (motion, person, manual…).

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Motion,
    Person,
    Vehicle,
    Animal,
    Onvif,
    /// A manual recording (spans the recording).
    Manual,
    /// A scheduled recording (spans the recording).
    Scheduled,
    Api,
    /// The camera was unreachable (spans the outage).
    CameraOffline,
    /// The camera came back after an outage (instant).
    CameraOnline,
    /// The camera reported a sign-in with a wrong password (instant).
    Security,
}

impl EventType {
    pub const ALL: [Self; 11] = [
        Self::Motion,
        Self::Person,
        Self::Vehicle,
        Self::Animal,
        Self::Onvif,
        Self::Manual,
        Self::Scheduled,
        Self::Api,
        Self::CameraOffline,
        Self::CameraOnline,
        Self::Security,
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub label: String,
    /// 0..1
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: Id,
    pub camera_id: Id,
    pub kind: EventType,
    pub start_time: Timestamp,
    /// `None` while the event is still in progress.
    pub end_time: Option<Timestamp>,
    /// Seconds
    pub duration: u32,
    pub recording_id: Option<Id>,
    pub thumbnail: Option<String>,
    pub protected: bool,
    pub detections: Vec<Detection>,
    /// What raised the event, e.g. "ONVIF RuleEngine/CellMotionDetector".
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventQuery {
    pub camera_id: Option<Id>,
    /// Empty = all types.
    pub kinds: Vec<EventType>,
    /// Inclusive.
    pub from: Option<Timestamp>,
    /// Exclusive.
    pub to: Option<Timestamp>,
    /// Only events starting within these local hours of the day, `[start, end)`,
    /// in the server's configured time zone. `(22, 6)` wraps past midnight.
    pub hours: Option<(u8, u8)>,
    /// Seconds.
    pub min_duration: Option<u32>,
    pub protected_only: bool,
    /// Page size; results are newest first.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// One page of events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    pub events: Vec<Event>,
    /// Matches in total, across all pages.
    pub total: u32,
}

/// An event with everything its details page needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDetail {
    pub event: Event,
    pub recording: Option<crate::Recording>,
    /// The event just before this one (any camera).
    pub previous: Option<Id>,
    /// The event just after this one (any camera).
    pub next: Option<Id>,
}
