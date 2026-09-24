//! Small data-fetching layer on top of Leptos resources.
//!
//! Each query belongs to a [`Topic`]. Mutations call [`invalidate`] on the
//! topics they affect, and every mounted query of that topic refetches while
//! keeping its previous value on screen. Later, backend push messages
//! (WebSocket) will invalidate topics the same way.

use std::cell::OnceCell;
use std::future::Future;
use std::time::Duration;

use leptos::prelude::*;

use super::ApiResult;
use crate::clock::use_interval;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Server,
    System,
    Notifications,
    Cameras,
    Events,
    Recordings,
    Storage,
    Settings,
}

impl Topic {
    const COUNT: usize = 8;
}

/// One revision counter per topic. Held outside the reactive context so
/// mutations can invalidate from async tasks (which have no owner after an
/// `.await`).
#[derive(Clone, Copy)]
struct Revisions([RwSignal<u64>; Topic::COUNT]);

thread_local! {
    static REVISIONS: OnceCell<Revisions> = const { OnceCell::new() };
}

/// Create the revision counters. Call once from the root component.
pub fn provide_queries() {
    REVISIONS.with(|r| {
        let _ = r.set(Revisions(std::array::from_fn(|_| RwSignal::new(0))));
    });
}

fn revision(topic: Topic) -> RwSignal<u64> {
    REVISIONS.with(|r| r.get().expect("provide_queries() must run before queries are used").0[topic as usize])
}

/// Refetch every query of `topic`.
pub fn invalidate(topic: Topic) {
    revision(topic).update(|r| *r += 1);
}

/// Fetch data for `topic`, refetching on invalidation and optionally on an interval.
pub fn use_query<T, Fut>(
    topic: Topic,
    poll: Option<Duration>,
    fetch: impl Fn() -> Fut + 'static,
) -> LocalResource<ApiResult<T>>
where
    T: 'static,
    Fut: Future<Output = ApiResult<T>> + 'static,
{
    let rev = revision(topic);
    if let Some(every) = poll {
        use_interval(every, move || rev.update(|r| *r += 1));
    }
    LocalResource::new(move || {
        rev.track();
        fetch()
    })
}
