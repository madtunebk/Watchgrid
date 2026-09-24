//! Group events by local calendar day for the list headings.

use chrono::{Duration, Local, NaiveDate};

use crate::api::Event;

pub fn by_day(events: Vec<Event>) -> Vec<(NaiveDate, Vec<Event>)> {
    let mut groups: Vec<(NaiveDate, Vec<Event>)> = Vec::new();
    for e in events {
        let day = e.start_time.with_timezone(&Local).date_naive();
        match groups.last_mut() {
            Some((d, list)) if *d == day => list.push(e),
            _ => groups.push((day, vec![e])),
        }
    }
    groups
}

/// "Today", "Yesterday", or "Monday, 22 September".
pub fn heading(day: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if day == today {
        "Today".into()
    } else if day == today - Duration::days(1) {
        "Yesterday".into()
    } else {
        day.format("%A, %-d %B").to_string()
    }
}
