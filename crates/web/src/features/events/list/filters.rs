//! Event filters: parsed from and written to the URL query string, so a
//! filtered list can be bookmarked, shared, and returned to from details.

use chrono::NaiveDate;
use leptos_router::params::ParamsMap;

use crate::api::{EventQuery, EventType, NvrDays};
use crate::features::events::labels;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    #[default]
    Today,
    Yesterday,
    Week,
    All,
    Day(NaiveDate),
}

impl Range {
    fn key(self) -> String {
        match self {
            Self::Today => "today".into(),
            Self::Yesterday => "yesterday".into(),
            Self::Week => "7d".into(),
            Self::All => "all".into(),
            Self::Day(d) => d.format("%Y-%m-%d").to_string(),
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "yesterday" => Self::Yesterday,
            "7d" => Self::Week,
            "all" => Self::All,
            other => NaiveDate::parse_from_str(other, "%Y-%m-%d").map(Self::Day).unwrap_or(Self::Today),
        }
    }
}

/// Time-of-day presets (local hours, end exclusive).
pub const HOURS: &[(&str, &str, (u8, u8))] = &[
    ("night", "Night 22–06", (22, 6)),
    ("morning", "Morning 06–12", (6, 12)),
    ("afternoon", "Afternoon 12–18", (12, 18)),
    ("evening", "Evening 18–22", (18, 22)),
];

/// Minimum-duration presets in seconds.
pub const DURATIONS: &[(u32, &str)] = &[(10, "≥ 10 s"), (30, "≥ 30 s"), (60, "≥ 1 min"), (300, "≥ 5 min")];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filters {
    pub camera: Option<String>,
    pub kinds: Vec<EventType>,
    pub range: Range,
    /// Key into [`HOURS`].
    pub hours: Option<&'static str>,
    pub min_duration: Option<u32>,
    pub protected_only: bool,
}

impl Filters {
    pub fn from_query(q: &ParamsMap) -> Self {
        Self {
            camera: q.get("camera").filter(|c| !c.is_empty()),
            kinds: q.get("types").unwrap_or_default().split(',').filter_map(labels::from_key).collect(),
            range: q.get("range").map(|r| Range::parse(&r)).unwrap_or_default(),
            hours: q.get("hours").and_then(|h| HOURS.iter().find(|(k, ..)| *k == h).map(|(k, ..)| *k)),
            min_duration: q.get("min").and_then(|m| m.parse().ok()),
            protected_only: q.get("protected").as_deref() == Some("1"),
        }
    }

    /// `/events?...` for these filters (defaults omitted).
    pub fn to_url(&self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = &self.camera {
            parts.push(format!("camera={c}"));
        }
        if !self.kinds.is_empty() {
            parts.push(format!("types={}", self.kinds.iter().map(|k| labels::css(*k)).collect::<Vec<_>>().join(",")));
        }
        if self.range != Range::Today {
            parts.push(format!("range={}", self.range.key()));
        }
        if let Some(h) = self.hours {
            parts.push(format!("hours={h}"));
        }
        if let Some(m) = self.min_duration {
            parts.push(format!("min={m}"));
        }
        if self.protected_only {
            parts.push("protected=1".into());
        }
        if parts.is_empty() { "/events".into() } else { format!("/events?{}", parts.join("&")) }
    }

    /// Anything besides the default date range?
    pub fn is_narrowed(&self) -> bool {
        Self { range: self.range, ..Default::default() } != *self
    }

    /// The server query for one page of `per_page` events (`page` from 0).
    pub fn to_query(&self, page: u32, per_page: u32) -> EventQuery {
        // Days are the NVR's (its time zone), worked out by the server.
        let days = match self.range {
            Range::Today => Some(NvrDays::Today),
            Range::Yesterday => Some(NvrDays::Yesterday),
            Range::Week => Some(NvrDays::Week),
            Range::All => None,
            Range::Day(d) => Some(NvrDays::Date(d)),
        };
        EventQuery {
            camera_id: self.camera.clone(),
            kinds: self.kinds.clone(),
            days,
            from: None,
            to: None,
            hours: self.hours.and_then(|k| HOURS.iter().find(|(key, ..)| *key == k)).map(|(.., h)| *h),
            min_duration: self.min_duration,
            protected_only: self.protected_only,
            limit: Some(per_page),
            offset: Some(page * per_page),
        }
    }
}
