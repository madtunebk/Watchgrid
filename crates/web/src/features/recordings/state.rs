//! Page state in the URL: view, day, cameras, zoom. Bookmarkable and
//! restored by Back.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use leptos_router::params::ParamsMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Timeline,
    Clips,
}

pub const ZOOMS: [u8; 5] = [1, 2, 4, 8, 24];

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub view: View,
    pub date: NaiveDate,
    /// Empty = all cameras.
    pub cameras: Vec<String>,
    pub zoom: u8,
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn local_midnight(d: NaiveDate) -> DateTime<Utc> {
    d.and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(Local).earliest())
        .map(|t| t.to_utc())
        .unwrap_or_else(Utc::now)
}

impl State {
    pub fn from_query(q: &ParamsMap) -> Self {
        Self {
            view: if q.get("view").as_deref() == Some("clips") { View::Clips } else { View::Timeline },
            date: q.get("date").and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()).unwrap_or_else(today),
            cameras: q.get("cameras").unwrap_or_default().split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
            zoom: q.get("zoom").and_then(|z| z.parse().ok()).filter(|z| ZOOMS.contains(z)).unwrap_or(1),
        }
    }

    pub fn to_url(&self) -> String {
        let mut parts = Vec::new();
        if self.view == View::Clips {
            parts.push("view=clips".to_string());
        }
        if self.date != today() {
            parts.push(format!("date={}", self.date.format("%Y-%m-%d")));
        }
        if !self.cameras.is_empty() {
            parts.push(format!("cameras={}", self.cameras.join(",")));
        }
        if self.zoom != 1 {
            parts.push(format!("zoom={}", self.zoom));
        }
        if parts.is_empty() { "/recordings".into() } else { format!("/recordings?{}", parts.join("&")) }
    }

    /// `[start, end)` of the selected local day (DST-safe).
    pub fn day_range(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        (local_midnight(self.date), local_midnight(self.date + Duration::days(1)))
    }

    pub fn shows(&self, camera_id: &str) -> bool {
        self.cameras.is_empty() || self.cameras.iter().any(|c| c == camera_id)
    }
}
