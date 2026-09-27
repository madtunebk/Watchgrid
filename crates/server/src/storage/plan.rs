//! Which recordings the retention policy deletes. Pure logic, no I/O.

use chrono::{DateTime, Duration, Utc};
use watchgrid_model::RetentionPolicy;

/// A recording retention may delete (protected ones are never candidates).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub bytes: u64,
    pub end_time: DateTime<Utc>,
    /// The camera's own age limit, if it has one.
    pub camera_max_days: Option<u32>,
    /// Folder the file is stored under; `None` = the default one.
    pub root: Option<String>,
    /// Filesystem holding the file (device id), when known.
    pub volume: Option<u64>,
}

/// State of the storage when planning.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    /// All recordings, protected included.
    pub recordings_bytes: u64,
    /// Free space per volume holding recordings, where known.
    pub free: Vec<(u64, u64)>,
}

/// Ids to delete, oldest first. `candidates` must be sorted oldest first.
///
/// Rules, each applied on top of the previous:
/// 0. each camera's own age limit (Camera → Storage);
/// 1. `max_age_days`: everything that ended longer ago than that;
/// 2. `max_usage`: oldest first until recordings fit the limit;
/// 3. `min_free`: on each volume, oldest first until it has that much free
///    space — only files on that volume count.
///
/// A recording chosen by several rules is counted once.
pub fn plan(policy: &RetentionPolicy, candidates: &[Candidate], usage: Usage, now: DateTime<Utc>) -> Vec<String> {
    let mut chosen = vec![false; candidates.len()];
    let mut freed = 0u64;
    // Marks a recording once, counting its bytes once.
    let choose = |i: usize, chosen: &mut [bool], freed: &mut u64| {
        if !chosen[i] {
            chosen[i] = true;
            *freed += candidates[i].bytes;
        }
    };

    for (i, c) in candidates.iter().enumerate() {
        if c.camera_max_days.is_some_and(|days| c.end_time < now - Duration::days(i64::from(days))) {
            choose(i, &mut chosen, &mut freed);
        }
    }
    if let Some(days) = policy.max_age_days {
        let cutoff = now - Duration::days(i64::from(days));
        for (i, c) in candidates.iter().enumerate() {
            if c.end_time < cutoff {
                choose(i, &mut chosen, &mut freed);
            }
        }
    }
    if let Some(limit) = policy.max_usage {
        for i in 0..candidates.len() {
            if usage.recordings_bytes.saturating_sub(freed) <= limit {
                break;
            }
            choose(i, &mut chosen, &mut freed);
        }
    }
    if let Some(min_free) = policy.min_free {
        for &(volume, free) in &usage.free {
            let on_volume = |i: usize| candidates[i].volume == Some(volume);
            let mut freed_here: u64 = (0..candidates.len()).filter(|&i| chosen[i] && on_volume(i)).map(|i| candidates[i].bytes).sum();
            for i in (0..candidates.len()).filter(|&i| on_volume(i)) {
                if free + freed_here >= min_free {
                    break;
                }
                if !chosen[i] {
                    choose(i, &mut chosen, &mut freed);
                    freed_here += candidates[i].bytes;
                }
            }
        }
    }

    candidates.iter().zip(chosen).filter(|(_, c)| *c).map(|(c, _)| c.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn now() -> DateTime<Utc> {
        "2026-09-30T12:00:00Z".parse().unwrap()
    }

    /// Four 1 GB recordings that ended 10, 5, 2 and 0 days ago.
    fn candidates() -> Vec<Candidate> {
        [10, 5, 2, 0].iter().enumerate().map(|(i, d)| Candidate { id: format!("r{i}"), bytes: GB, end_time: now() - Duration::days(*d), camera_max_days: None, root: None, volume: Some(1) }).collect()
    }

    fn policy(days: Option<u32>, max: Option<u64>, free: Option<u64>) -> RetentionPolicy {
        RetentionPolicy { max_age_days: days, max_usage: max, min_free: free, event_history_days: None }
    }

    fn usage(recordings_bytes: u64, free: u64) -> Usage {
        Usage { recordings_bytes, free: vec![(1, free)] }
    }

    #[test]
    fn no_rules_delete_nothing() {
        assert!(plan(&policy(None, None, None), &candidates(), usage(4 * GB, 0), now()).is_empty());
    }

    #[test]
    fn age_deletes_only_older_recordings() {
        assert_eq!(plan(&policy(Some(3), None, None), &candidates(), usage(4 * GB, 100 * GB), now()), ["r0", "r1"]);
    }

    #[test]
    fn usage_limit_deletes_oldest_until_it_fits() {
        // 4 GB of recordings, 2.5 GB allowed: drop two.
        assert_eq!(plan(&policy(None, Some(2 * GB + GB / 2), None), &candidates(), usage(4 * GB, 100 * GB), now()), ["r0", "r1"]);
    }

    #[test]
    fn protected_bytes_count_towards_the_limit() {
        // 2 GB more (protected) are not candidates but still use space.
        assert_eq!(plan(&policy(None, Some(3 * GB), None), &candidates(), usage(6 * GB, 100 * GB), now()), ["r0", "r1", "r2"]);
    }

    #[test]
    fn free_space_rule_counts_what_other_rules_freed() {
        // Age frees r0 (1 GB); 1 GB free + 1 GB = 2 GB, need 3 GB: also r1.
        assert_eq!(plan(&policy(Some(7), None, Some(3 * GB)), &candidates(), usage(4 * GB, GB), now()), ["r0", "r1"]);
    }

    #[test]
    fn unknown_free_space_skips_that_rule() {
        let u = Usage { recordings_bytes: 4 * GB, free: vec![] };
        assert!(plan(&policy(None, None, Some(100 * GB)), &candidates(), u, now()).is_empty());
    }

    #[test]
    fn cannot_delete_more_than_exists() {
        assert_eq!(plan(&policy(None, None, Some(1000 * GB)), &candidates(), usage(4 * GB, 0), now()).len(), 4);
    }

    #[test]
    fn a_camera_limit_applies_to_that_camera_only() {
        let mut c = candidates();
        c[1].camera_max_days = Some(3); // 5 days old, camera keeps 3
        c[2].camera_max_days = Some(3); // 2 days old: kept
        assert_eq!(plan(&policy(None, None, None), &c, Usage { recordings_bytes: 4 * GB, free: vec![] }, now()), ["r1"]);
        // Together with the global limit.
        assert_eq!(plan(&policy(Some(7), None, None), &c, Usage { recordings_bytes: 4 * GB, free: vec![] }, now()), ["r0", "r1"]);
    }

    #[test]
    fn a_recording_chosen_by_two_rules_frees_its_bytes_once() {
        // r0 expires by the camera's and the global age limit; the usage
        // limit (2.5 GB of 4) must still take r1 as well.
        let mut c = candidates();
        c[0].camera_max_days = Some(3);
        assert_eq!(plan(&policy(Some(7), Some(2 * GB + GB / 2), None), &c, usage(4 * GB, 100 * GB), now()), ["r0", "r1"]);
    }

    #[test]
    fn free_space_is_made_on_the_volume_that_needs_it() {
        // r0, r1 on volume 1 (plenty free); r2, r3 on volume 2 (full).
        let mut c = candidates();
        c[2].volume = Some(2);
        c[3].volume = Some(2);
        let u = Usage { recordings_bytes: 4 * GB, free: vec![(1, 100 * GB), (2, 0)] };
        assert_eq!(plan(&policy(None, None, Some(GB)), &c, u, now()), ["r2"], "deleting on volume 1 would not help volume 2");
    }
}
