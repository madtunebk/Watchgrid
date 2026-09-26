//! Changing many events at once. The same plan is the preview (shown in
//! the confirmation) and the work list; applying re-checks everything under
//! the clip locks, so what changed since the preview is skipped, not forced.

use std::collections::{BTreeSet, HashSet};

use sqlx::PgPool;
use watchgrid_model::{BulkSkip, BulkSkipReason, EventBulkAction, EventBulkSummary};

use crate::recordings::{self, DeleteError};
use crate::state::AppState;

/// An event as the plan needs it.
#[derive(Debug, Clone)]
pub(super) struct EventRow {
    pub id: String,
    pub recording_id: Option<String>,
    pub protected: bool,
    pub ended: bool,
}

/// A saved recording of a selected event, with all its events.
#[derive(Debug, Clone)]
pub struct ClipRow {
    pub id: String,
    pub bytes: u64,
    /// By hand or by any event.
    pub protected: bool,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct Plan {
    pub events: Vec<String>,
    pub skipped_events: Vec<BulkSkip>,
    /// (id, bytes)
    pub recordings: Vec<(String, u64)>,
    pub kept_recordings: Vec<BulkSkip>,
}

impl Plan {
    fn summary(&self) -> EventBulkSummary {
        EventBulkSummary {
            events: self.events.len() as u32,
            recordings: self.recordings.len() as u32,
            bytes: self.recordings.iter().map(|(_, b)| b).sum(),
            skipped_events: self.skipped_events.clone(),
            kept_recordings: self.kept_recordings.clone(),
        }
    }
}

fn skip(id: &str, reason: BulkSkipReason) -> BulkSkip {
    BulkSkip { id: id.to_string(), reason }
}

/// What the action does to these events. `recording(id)` tells whether a
/// clip is still being written. Pure: no I/O.
pub(super) fn decide(action: EventBulkAction, requested: &[String], events: &[EventRow], clips: &[ClipRow], recording: &dyn Fn(&str) -> bool) -> Plan {
    let mut plan = Plan::default();
    let mut seen = HashSet::new();
    for id in requested.iter().filter(|id| seen.insert(id.as_str())) {
        let Some(e) = events.iter().find(|e| &e.id == id) else {
            plan.skipped_events.push(skip(id, BulkSkipReason::NotFound));
            continue;
        };
        match action {
            EventBulkAction::Protect if !e.protected => plan.events.push(e.id.clone()),
            EventBulkAction::Unprotect if e.protected => plan.events.push(e.id.clone()),
            EventBulkAction::Protect | EventBulkAction::Unprotect => {} // already so
            EventBulkAction::Delete | EventBulkAction::DeleteWithVideo => {
                if e.protected {
                    plan.skipped_events.push(skip(id, BulkSkipReason::Protected));
                } else if !e.ended && e.recording_id.is_some() {
                    plan.skipped_events.push(skip(id, BulkSkipReason::InProgress));
                } else {
                    plan.events.push(e.id.clone());
                }
            }
        }
    }
    if action != EventBulkAction::DeleteWithVideo {
        return plan;
    }

    let deleted: HashSet<&str> = plan.events.iter().map(String::as_str).collect();
    let wanted: BTreeSet<&str> = events.iter().filter(|e| deleted.contains(e.id.as_str())).filter_map(|e| e.recording_id.as_deref()).collect();
    for rid in wanted {
        if recording(rid) {
            plan.kept_recordings.push(skip(rid, BulkSkipReason::Recording));
            continue;
        }
        let Some(clip) = clips.iter().find(|c| c.id == rid) else { continue }; // already gone
        if clip.protected {
            plan.kept_recordings.push(skip(rid, BulkSkipReason::Protected));
        } else if clip.event_ids.iter().any(|e| !deleted.contains(e.as_str())) {
            plan.kept_recordings.push(skip(rid, BulkSkipReason::Shared));
        } else {
            plan.recordings.push((clip.id.clone(), clip.bytes));
        }
    }
    plan
}

async fn load(s: &AppState, action: EventBulkAction, ids: &[String]) -> sqlx::Result<Plan> {
    let events = super::repo::bulk_rows(&s.db, ids).await?;
    let clips = if action == EventBulkAction::DeleteWithVideo {
        let rids: Vec<String> = events.iter().filter_map(|e| e.recording_id.clone()).collect();
        recordings::clips_with_events(&s.db, &rids).await?
    } else {
        Vec::new()
    };
    let live = s.recorder.live();
    Ok(decide(action, ids, &events, &clips, &|id| live.get(id).is_some()))
}

pub(super) async fn preview(s: &AppState, action: EventBulkAction, ids: &[String]) -> sqlx::Result<EventBulkSummary> {
    Ok(load(s, action, ids).await?.summary())
}

/// Do it. The summary says what actually happened.
pub(super) async fn apply(s: &AppState, action: EventBulkAction, ids: &[String]) -> sqlx::Result<EventBulkSummary> {
    let plan = load(s, action, ids).await?;
    let mut done = EventBulkSummary { skipped_events: plan.skipped_events.clone(), kept_recordings: plan.kept_recordings.clone(), ..Default::default() };
    match action {
        EventBulkAction::Protect | EventBulkAction::Unprotect => {
            let protect = action == EventBulkAction::Protect;
            for id in &plan.events {
                set_protected(&s.db, id, protect).await?;
                done.events += 1;
                if protect {
                    s.exports.on_protected(id).await;
                }
            }
        }
        EventBulkAction::Delete | EventBulkAction::DeleteWithVideo => {
            for (rid, bytes) in &plan.recordings {
                match recordings::delete_recording(&s.db, &s.recording_files, rid).await {
                    Ok(true) => {
                        done.recordings += 1;
                        done.bytes += bytes;
                    }
                    Ok(false) => {}
                    Err(DeleteError::Protected) => done.kept_recordings.push(skip(rid, BulkSkipReason::Protected)),
                    Err(DeleteError::Exporting) => done.kept_recordings.push(skip(rid, BulkSkipReason::Exporting)),
                    Err(DeleteError::Failed(e)) => {
                        tracing::warn!(recording = %rid, "bulk delete: {e}");
                        done.kept_recordings.push(skip(rid, BulkSkipReason::Failed));
                    }
                }
            }
            let gone = super::repo::delete_many(&s.db, &plan.events).await?;
            done.events = gone.len() as u32;
            // Protected or started meanwhile.
            done.skipped_events.extend(plan.events.iter().filter(|id| !gone.contains(id)).map(|id| skip(id, BulkSkipReason::Protected)));
        }
    }
    Ok(done)
}

/// Same as protecting one event: under its clip's lock.
async fn set_protected(db: &PgPool, id: &str, protected: bool) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    if let Some(rid) = super::repo::get(db, id).await?.and_then(|e| e.recording_id) {
        recordings::lock_clip(&mut tx, &rid).await?;
    }
    super::repo::set_protected(&mut *tx, id, protected).await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, rec: Option<&str>, protected: bool, ended: bool) -> EventRow {
        EventRow { id: id.into(), recording_id: rec.map(Into::into), protected, ended }
    }

    fn clip(id: &str, protected: bool, events: &[&str]) -> ClipRow {
        ClipRow { id: id.into(), bytes: 100, protected, event_ids: events.iter().map(|e| e.to_string()).collect() }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn reasons(v: &[BulkSkip]) -> Vec<(&str, BulkSkipReason)> {
        v.iter().map(|s| (s.id.as_str(), s.reason)).collect()
    }

    const NOT_RECORDING: &dyn Fn(&str) -> bool = &|_| false;

    #[test]
    fn delete_skips_protected_and_running_events() {
        let events = [ev("a", Some("r1"), false, true), ev("b", Some("r1"), true, true), ev("c", Some("r2"), false, false), ev("d", None, false, false)];
        let plan = decide(EventBulkAction::Delete, &ids(&["a", "b", "c", "d", "x", "a"]), &events, &[], NOT_RECORDING);
        assert_eq!(plan.events, ["a", "d"], "an open outage without a clip can go; duplicates count once");
        assert_eq!(reasons(&plan.skipped_events), [("b", BulkSkipReason::Protected), ("c", BulkSkipReason::InProgress), ("x", BulkSkipReason::NotFound)]);
        assert!(plan.recordings.is_empty(), "plain delete keeps every video");
    }

    #[test]
    fn video_goes_only_when_all_its_events_go_and_nothing_protects_it() {
        let events = [ev("a", Some("r1"), false, true), ev("b", Some("r1"), false, true), ev("c", Some("r2"), false, true), ev("d", Some("r3"), false, true), ev("e", Some("r4"), false, true)];
        let clips = [clip("r1", false, &["a", "b"]), clip("r2", false, &["c", "other"]), clip("r3", true, &["d"])];
        let recording = |id: &str| id == "r4";
        let plan = decide(EventBulkAction::DeleteWithVideo, &ids(&["a", "b", "c", "d", "e"]), &events, &clips, &recording);
        assert_eq!(plan.recordings, [("r1".to_string(), 100)]);
        assert_eq!(
            reasons(&plan.kept_recordings),
            [("r2", BulkSkipReason::Shared), ("r3", BulkSkipReason::Protected), ("r4", BulkSkipReason::Recording)]
        );
        assert_eq!(plan.events.len(), 5, "the events themselves are all deleted");
    }

    #[test]
    fn a_protected_event_keeps_its_clip_even_if_the_others_go() {
        let events = [ev("a", Some("r1"), false, true), ev("b", Some("r1"), true, true)];
        let clips = [clip("r1", true, &["a", "b"])];
        let plan = decide(EventBulkAction::DeleteWithVideo, &ids(&["a", "b"]), &events, &clips, NOT_RECORDING);
        assert_eq!(plan.events, ["a"]);
        assert_eq!(reasons(&plan.kept_recordings), [("r1", BulkSkipReason::Protected)]);
    }

    #[test]
    fn protect_and_unprotect_change_only_what_differs() {
        let events = [ev("a", None, false, true), ev("b", None, true, true)];
        assert_eq!(decide(EventBulkAction::Protect, &ids(&["a", "b"]), &events, &[], NOT_RECORDING).events, ["a"]);
        assert_eq!(decide(EventBulkAction::Unprotect, &ids(&["a", "b"]), &events, &[], NOT_RECORDING).events, ["b"]);
    }

    mod db {
        use chrono::{Duration, Utc};
        use sqlx::PgPool;
        use watchgrid_model::{BulkSkipReason, EventBulkAction, RecordingReason};

        use super::super::apply;
        use crate::credentials::CredentialStore;
        use crate::recordings::{self, NewRecording};
        use crate::state::AppState;

        /// `test` keeps the files of tests running side by side apart.
        async fn clip(s: &AppState, test: &str, id: &str) -> std::path::PathBuf {
            let path = format!("bulk-{}-{test}/{id}.mp4", std::process::id());
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
            recordings::insert(&s.db, &row).await.unwrap();
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
        async fn deleting_with_video_keeps_clips_shared_with_other_events(db: PgPool) {
            let s = AppState::inert(db.clone(), CredentialStore::from_key(&[3u8; 32]));
            let shared = clip(&s, "shared", "r1").await;
            let alone = clip(&s, "shared", "r2").await;
            event(&db, "a", "r1").await;
            event(&db, "b", "r1").await; // not selected
            event(&db, "c", "r2").await;

            let ids = vec!["a".to_string(), "c".to_string()];
            let done = apply(&s, EventBulkAction::DeleteWithVideo, &ids).await.unwrap();
            assert_eq!((done.events, done.recordings, done.bytes), (2, 1, 5));
            assert_eq!(done.kept_recordings.iter().map(|k| (k.id.as_str(), k.reason)).collect::<Vec<_>>(), [("r1", BulkSkipReason::Shared)]);
            assert!(shared.exists() && !alone.exists());
            let left: Vec<String> = sqlx::query_scalar("SELECT id FROM events ORDER BY id").fetch_all(&db).await.unwrap();
            assert_eq!(left, ["b"]);
            let _ = std::fs::remove_file(shared);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn bulk_protect_keeps_the_clip(db: PgPool) {
            let s = AppState::inert(db.clone(), CredentialStore::from_key(&[3u8; 32]));
            let file = clip(&s, "protect", "r1").await;
            event(&db, "a", "r1").await;
            let done = apply(&s, EventBulkAction::Protect, &["a".to_string()]).await.unwrap();
            assert_eq!(done.events, 1);
            let done = apply(&s, EventBulkAction::DeleteWithVideo, &["a".to_string()]).await.unwrap();
            assert_eq!((done.events, done.recordings), (0, 0), "a protected event is skipped");
            assert!(file.exists());
            let _ = std::fs::remove_file(file);
        }
    }
}
