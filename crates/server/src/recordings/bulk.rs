//! Changing many recordings at once. Like events' bulk actions: one plan
//! is both the preview and the work list, and each delete re-checks the
//! protection under the clip's lock (via `delete_recording`).

use std::collections::HashSet;

use watchgrid_model::{BulkSkip, BulkSkipReason, RecordingBulkAction, RecordingBulkSummary};

use super::{DeleteError, delete_recording, repo};
use crate::state::AppState;

/// A saved recording as the plan needs it.
#[derive(Debug, Clone)]
pub struct ClipState {
    pub id: String,
    pub bytes: u64,
    /// Protected by hand.
    pub protected: bool,
    /// Protected events in it.
    pub protected_by_events: u32,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct Plan {
    /// (id, bytes, events)
    pub recordings: Vec<(String, u64, Vec<String>)>,
    pub skipped: Vec<BulkSkip>,
}

fn skip(id: &str, reason: BulkSkipReason) -> BulkSkip {
    BulkSkip { id: id.to_string(), reason }
}

/// What the action does to these recordings. `recording(id)` tells whether
/// a clip is still being written (those are not in `rows` yet). Pure.
pub(super) fn decide(action: RecordingBulkAction, requested: &[String], rows: &[ClipState], recording: &dyn Fn(&str) -> bool) -> Plan {
    let mut plan = Plan::default();
    let mut seen = HashSet::new();
    for id in requested.iter().filter(|id| seen.insert(id.as_str())) {
        let Some(r) = rows.iter().find(|r| &r.id == id) else {
            plan.skipped.push(skip(id, if recording(id) { BulkSkipReason::Recording } else { BulkSkipReason::NotFound }));
            continue;
        };
        let take = || (r.id.clone(), r.bytes, r.event_ids.clone());
        match action {
            RecordingBulkAction::Protect if !r.protected => plan.recordings.push(take()),
            RecordingBulkAction::Unprotect => {
                if r.protected {
                    plan.recordings.push(take());
                }
                // Unprotecting by hand leaves the events' protection.
                if r.protected_by_events > 0 {
                    plan.skipped.push(skip(id, BulkSkipReason::Protected));
                }
            }
            RecordingBulkAction::Protect => {}
            RecordingBulkAction::Delete | RecordingBulkAction::DeleteWithEvents => {
                if r.protected || r.protected_by_events > 0 {
                    plan.skipped.push(skip(id, BulkSkipReason::Protected));
                } else {
                    plan.recordings.push(take());
                }
            }
        }
    }
    plan
}

fn summary(action: RecordingBulkAction, plan: &Plan) -> RecordingBulkSummary {
    let deleting = matches!(action, RecordingBulkAction::Delete | RecordingBulkAction::DeleteWithEvents);
    RecordingBulkSummary {
        recordings: plan.recordings.len() as u32,
        bytes: if deleting { plan.recordings.iter().map(|(_, b, _)| b).sum() } else { 0 },
        events: if action == RecordingBulkAction::DeleteWithEvents { plan.recordings.iter().map(|(_, _, e)| e.len() as u32).sum() } else { 0 },
        skipped: plan.skipped.clone(),
    }
}

async fn load(s: &AppState, action: RecordingBulkAction, ids: &[String]) -> sqlx::Result<Plan> {
    let rows = repo::bulk_rows(&s.db, ids).await?;
    let live = s.recorder.live();
    Ok(decide(action, ids, &rows, &|id| live.get(id).is_some()))
}

pub(super) async fn preview(s: &AppState, action: RecordingBulkAction, ids: &[String]) -> sqlx::Result<RecordingBulkSummary> {
    Ok(summary(action, &load(s, action, ids).await?))
}

/// Do it. The summary says what actually happened.
pub(super) async fn apply(s: &AppState, action: RecordingBulkAction, ids: &[String]) -> sqlx::Result<RecordingBulkSummary> {
    let plan = load(s, action, ids).await?;
    let mut done = RecordingBulkSummary { skipped: plan.skipped.clone(), ..Default::default() };
    match action {
        RecordingBulkAction::Protect | RecordingBulkAction::Unprotect => {
            let protect = action == RecordingBulkAction::Protect;
            for (id, _, _) in &plan.recordings {
                repo::set_protected(&s.db, id, protect).await?;
                done.recordings += 1;
                if protect {
                    s.exports.on_recording_protected(id, None).await;
                }
            }
        }
        RecordingBulkAction::Delete | RecordingBulkAction::DeleteWithEvents => {
            let mut events = Vec::new();
            for (id, bytes, event_ids) in &plan.recordings {
                match delete_recording(&s.db, &s.recording_files, id).await {
                    Ok(true) => {
                        done.recordings += 1;
                        done.bytes += bytes;
                        events.extend(event_ids.iter().cloned());
                    }
                    Ok(false) => done.skipped.push(skip(id, BulkSkipReason::NotFound)),
                    Err(DeleteError::Protected) => done.skipped.push(skip(id, BulkSkipReason::Protected)),
                    Err(DeleteError::Failed(e)) => {
                        tracing::warn!(recording = %id, "bulk delete: {e}");
                        done.skipped.push(skip(id, BulkSkipReason::Failed));
                    }
                }
            }
            if action == RecordingBulkAction::DeleteWithEvents && !events.is_empty() {
                done.events = crate::events::delete_events(&s.db, &events).await?.len() as u32;
            }
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, protected: bool, by_events: u32, events: &[&str]) -> ClipState {
        ClipState { id: id.into(), bytes: 10, protected, protected_by_events: by_events, event_ids: events.iter().map(|e| e.to_string()).collect() }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn reasons(p: &Plan) -> Vec<(&str, BulkSkipReason)> {
        p.skipped.iter().map(|s| (s.id.as_str(), s.reason)).collect()
    }

    #[test]
    fn delete_skips_protected_and_running_clips() {
        let rows = [row("a", false, 0, &["e1", "e2"]), row("b", true, 0, &[]), row("c", false, 1, &["e3"])];
        let live = |id: &str| id == "live";
        let plan = decide(RecordingBulkAction::DeleteWithEvents, &ids(&["a", "b", "c", "live", "gone", "a"]), &rows, &live);
        assert_eq!(plan.recordings, [("a".to_string(), 10, ids(&["e1", "e2"]))]);
        assert_eq!(
            reasons(&plan),
            [("b", BulkSkipReason::Protected), ("c", BulkSkipReason::Protected), ("live", BulkSkipReason::Recording), ("gone", BulkSkipReason::NotFound)]
        );
        let s = summary(RecordingBulkAction::DeleteWithEvents, &plan);
        assert_eq!((s.recordings, s.bytes, s.events), (1, 10, 2));
        assert_eq!(summary(RecordingBulkAction::Delete, &plan).events, 0, "plain delete keeps the events");
    }

    #[test]
    fn unprotect_clears_the_manual_flag_and_reports_event_protection() {
        let rows = [row("a", true, 0, &[]), row("b", true, 2, &[]), row("c", false, 0, &[])];
        let plan = decide(RecordingBulkAction::Unprotect, &ids(&["a", "b", "c"]), &rows, &|_| false);
        assert_eq!(plan.recordings.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(reasons(&plan), [("b", BulkSkipReason::Protected)], "b stays protected by its events");
        let plan = decide(RecordingBulkAction::Protect, &ids(&["a", "c"]), &rows, &|_| false);
        assert_eq!(plan.recordings.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["c"]);
    }

    mod db {
        use chrono::{Duration, Utc};
        use sqlx::PgPool;
        use watchgrid_model::{RecordingBulkAction, RecordingReason};

        use super::super::apply;
        use crate::credentials::CredentialStore;
        use crate::recordings::{NewRecording, insert};
        use crate::state::AppState;

        async fn clip(s: &AppState, id: &str) -> std::path::PathBuf {
            let path = format!("rec-bulk-{}/{id}.mp4", std::process::id());
            let file = s.recording_files.resolve(None, &path).unwrap();
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"video").unwrap();
            let start = Utc::now() - Duration::hours(1);
            let row = NewRecording {
                id: id.into(),
                camera_id: "cam-a".into(),
                reason: RecordingReason::Motion,
                start_time: start,
                end_time: start + Duration::seconds(60),
                duration_ms: 60_000,
                file_size: 5,
                path,
                root: None,
                codec: "avc1.640028".into(),
                width: 1280,
                height: 720,
            };
            insert(&s.db, &row).await.unwrap();
            file
        }

        async fn event(db: &PgPool, id: &str, rec: &str) {
            sqlx::query("INSERT INTO events (id, camera_id, kind, start_time, end_time, recording_id, source) VALUES ($1, 'cam-a', 'motion', now(), now(), $2, 'test')")
                .bind(id)
                .bind(rec)
                .execute(db)
                .await
                .unwrap();
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn deleting_with_events_removes_both_and_plain_delete_keeps_events(db: PgPool) {
            let s = AppState::inert(db.clone(), CredentialStore::from_key(&[3u8; 32]));
            let (f1, f2) = (clip(&s, "r1").await, clip(&s, "r2").await);
            event(&db, "a", "r1").await;
            event(&db, "b", "r1").await;
            event(&db, "c", "r2").await;

            let done = apply(&s, RecordingBulkAction::DeleteWithEvents, &["r1".to_string()]).await.unwrap();
            assert_eq!((done.recordings, done.bytes, done.events), (1, 5, 2));
            let done = apply(&s, RecordingBulkAction::Delete, &["r2".to_string()]).await.unwrap();
            assert_eq!((done.recordings, done.events), (1, 0));

            assert!(!f1.exists() && !f2.exists());
            let left: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, recording_id FROM events ORDER BY id").fetch_all(&db).await.unwrap();
            assert_eq!(left, [("c".to_string(), None)], "c stays, without video");
        }
    }
}
