//! Words for bulk actions on recordings: what will happen and what did.

use crate::api::{BulkSkipReason, RecordingBulkAction, RecordingBulkSummary};
use crate::format;

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn recordings(n: u32) -> String {
    plural(n, "recording", "recordings")
}

/// "3 recordings (1.2 GB)", plus " and 5 events" when events go too.
fn deleted(s: &RecordingBulkSummary) -> String {
    let mut out = format!("{} ({})", recordings(s.recordings), format::bytes(s.bytes));
    if s.events > 0 {
        out.push_str(&format!(" and {}", plural(s.events, "event", "events")));
    }
    out
}

/// Why some recordings were left alone, one line per reason.
pub fn left_alone(action: RecordingBulkAction, s: &RecordingBulkSummary) -> Vec<String> {
    let mut counts: Vec<(BulkSkipReason, u32)> = Vec::new();
    for k in &s.skipped {
        match counts.iter_mut().find(|(r, _)| *r == k.reason) {
            Some((_, n)) => *n += 1,
            None => counts.push((k.reason, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(reason, n)| match reason {
            BulkSkipReason::Protected if action == RecordingBulkAction::Unprotect => format!("{} still protected by protected events.", recordings(n)),
            BulkSkipReason::Protected => format!("{} skipped: protected.", recordings(n)),
            BulkSkipReason::Recording => format!("{} skipped: still recording.", recordings(n)),
            BulkSkipReason::NotFound => format!("{} skipped: already gone.", recordings(n)),
            BulkSkipReason::Failed => format!("{} kept: the file could not be deleted (see the log).", recordings(n)),
            BulkSkipReason::Exporting => format!("{} kept: an upload is still pending (Settings → Exports can cancel it).", recordings(n)),
            BulkSkipReason::Shared | BulkSkipReason::InProgress => format!("{} skipped.", recordings(n)),
        })
        .collect()
}

/// The confirmation's main sentence.
pub fn will(action: RecordingBulkAction, s: &RecordingBulkSummary) -> String {
    match action {
        _ if s.recordings == 0 => "Nothing to change.".into(),
        RecordingBulkAction::Delete => format!("{} will be deleted permanently. Their events stay in the history, without video.", deleted(s)),
        RecordingBulkAction::DeleteWithEvents => format!("{} will be deleted permanently.", deleted(s)),
        RecordingBulkAction::Protect => format!("{} will be protected.", recordings(s.recordings)),
        RecordingBulkAction::Unprotect => format!("{} will be unprotected.", recordings(s.recordings)),
    }
}

/// After the action: what happened, then what was left alone.
pub fn did(action: RecordingBulkAction, s: &RecordingBulkSummary) -> String {
    let main = match action {
        _ if s.recordings == 0 => "Nothing changed.".to_string(),
        RecordingBulkAction::Protect => format!("Protected {}.", recordings(s.recordings)),
        RecordingBulkAction::Unprotect => format!("Unprotected {}.", recordings(s.recordings)),
        RecordingBulkAction::Delete => format!("Deleted {}; their events stay without video.", deleted(s)),
        RecordingBulkAction::DeleteWithEvents => format!("Deleted {}.", deleted(s)),
    };
    std::iter::once(main).chain(left_alone(action, s)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::BulkSkip;

    fn skip(reason: BulkSkipReason) -> BulkSkip {
        BulkSkip { id: "x".into(), reason }
    }

    #[test]
    fn delete_with_events_names_both() {
        let s = RecordingBulkSummary { recordings: 2, bytes: 2_000_000, events: 3, skipped: vec![skip(BulkSkipReason::Protected), skip(BulkSkipReason::Recording)] };
        assert!(will(RecordingBulkAction::DeleteWithEvents, &s).ends_with("and 3 events will be deleted permanently."));
        assert_eq!(left_alone(RecordingBulkAction::DeleteWithEvents, &s), ["1 recording skipped: protected.", "1 recording skipped: still recording."]);
    }

    #[test]
    fn unprotect_explains_protection_by_events() {
        let s = RecordingBulkSummary { recordings: 1, skipped: vec![skip(BulkSkipReason::Protected)], ..Default::default() };
        assert_eq!(did(RecordingBulkAction::Unprotect, &s), "Unprotected 1 recording. 1 recording still protected by protected events.");
    }
}
