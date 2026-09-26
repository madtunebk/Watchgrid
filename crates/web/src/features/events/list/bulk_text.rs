//! Words for bulk actions: what will happen (confirmation) and what did.

use crate::api::{BulkSkip, BulkSkipReason, EventBulkAction, EventBulkSummary};
use crate::format;

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "12 events", "1 recording (1.2 GB)"
fn events(n: u32) -> String {
    plural(n, "event", "events")
}

fn recordings(s: &EventBulkSummary) -> String {
    format!("{} ({})", plural(s.recordings, "recording", "recordings"), format::bytes(s.bytes))
}

fn event_reason(r: BulkSkipReason) -> &'static str {
    match r {
        BulkSkipReason::Protected => "protected",
        BulkSkipReason::InProgress => "still going on",
        BulkSkipReason::NotFound => "already gone",
        BulkSkipReason::Shared | BulkSkipReason::Recording | BulkSkipReason::Failed | BulkSkipReason::Exporting => "could not be changed",
    }
}

fn recording_reason(r: BulkSkipReason) -> &'static str {
    match r {
        BulkSkipReason::Shared => "shared with events you didn't select",
        BulkSkipReason::Protected => "protected",
        BulkSkipReason::Recording => "still recording",
        BulkSkipReason::Failed => "the file could not be deleted (see the log)",
        BulkSkipReason::Exporting => "an upload of it is still pending",
        BulkSkipReason::InProgress | BulkSkipReason::NotFound => "not deleted",
    }
}

/// One line per reason, in the order reasons first appear.
fn grouped(list: &[BulkSkip], what: (&str, &str), verb: &str, reason: fn(BulkSkipReason) -> &'static str) -> Vec<String> {
    let mut out: Vec<(BulkSkipReason, u32)> = Vec::new();
    for s in list {
        match out.iter_mut().find(|(r, _)| *r == s.reason) {
            Some((_, n)) => *n += 1,
            None => out.push((s.reason, 1)),
        }
    }
    out.into_iter().map(|(r, n)| format!("{} {verb}: {}.", plural(n, what.0, what.1), reason(r))).collect()
}

/// Why some items were left alone.
pub fn left_alone(s: &EventBulkSummary) -> Vec<String> {
    let mut lines = grouped(&s.skipped_events, ("event", "events"), "skipped", event_reason);
    lines.extend(grouped(&s.kept_recordings, ("recording", "recordings"), "kept", recording_reason));
    lines
}

/// The confirmation's main sentence.
pub fn will(action: EventBulkAction, s: &EventBulkSummary) -> String {
    match action {
        _ if s.events == 0 => "Nothing to change.".into(),
        EventBulkAction::Delete => format!("{} will be removed from the history. Their recordings are kept.", events(s.events)),
        EventBulkAction::DeleteWithVideo if s.recordings == 0 => {
            format!("{} will be removed from the history. No recording can be deleted with them.", events(s.events))
        }
        EventBulkAction::DeleteWithVideo => format!("{} and {} will be deleted permanently.", events(s.events), recordings(s)),
        EventBulkAction::Protect => format!("{} will be protected.", events(s.events)),
        EventBulkAction::Unprotect => format!("{} will be unprotected.", events(s.events)),
    }
}

/// After the action: what happened, then what was left alone.
pub fn did(action: EventBulkAction, s: &EventBulkSummary) -> String {
    let main = match action {
        _ if s.events == 0 && s.recordings == 0 => "Nothing changed.".to_string(),
        EventBulkAction::Protect => format!("Protected {}.", events(s.events)),
        EventBulkAction::Unprotect => format!("Unprotected {}.", events(s.events)),
        EventBulkAction::Delete => format!("Deleted {}; their recordings are kept.", events(s.events)),
        EventBulkAction::DeleteWithVideo if s.recordings == 0 => format!("Deleted {}.", events(s.events)),
        EventBulkAction::DeleteWithVideo => format!("Deleted {} and {}.", events(s.events), recordings(s)),
    };
    std::iter::once(main).chain(left_alone(s)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skip(id: &str, reason: BulkSkipReason) -> BulkSkip {
        BulkSkip { id: id.into(), reason }
    }

    #[test]
    fn delete_with_video_explains_what_is_kept_and_why() {
        let s = EventBulkSummary {
            events: 14,
            recordings: 9,
            bytes: 3_200_000_000,
            skipped_events: vec![skip("e1", BulkSkipReason::Protected)],
            kept_recordings: vec![skip("r1", BulkSkipReason::Shared), skip("r2", BulkSkipReason::Shared), skip("r3", BulkSkipReason::Protected)],
        };
        assert!(will(EventBulkAction::DeleteWithVideo, &s).starts_with("14 events and 9 recordings ("));
        assert_eq!(
            left_alone(&s),
            ["1 event skipped: protected.", "2 recordings kept: shared with events you didn't select.", "1 recording kept: protected."]
        );
    }

    #[test]
    fn plain_delete_says_the_video_stays() {
        let s = EventBulkSummary { events: 1, ..Default::default() };
        assert_eq!(will(EventBulkAction::Delete, &s), "1 event will be removed from the history. Their recordings are kept.");
        assert_eq!(did(EventBulkAction::Delete, &s), "Deleted 1 event; their recordings are kept.");
    }

    #[test]
    fn nothing_to_do_is_said_plainly() {
        let s = EventBulkSummary { skipped_events: vec![skip("a", BulkSkipReason::InProgress)], ..Default::default() };
        assert_eq!(will(EventBulkAction::Delete, &s), "Nothing to change.");
        assert_eq!(did(EventBulkAction::Delete, &s), "Nothing changed. 1 event skipped: still going on.");
    }
}
