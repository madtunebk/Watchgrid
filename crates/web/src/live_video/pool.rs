//! Players outlive the widgets that show them by a few seconds, so
//! re-rendering a camera tile (or navigating between pages that show the
//! same camera) reuses the running stream instead of reconnecting.

use std::cell::RefCell;
use std::collections::HashMap;

use gloo_timers::callback::Timeout;
use leptos::prelude::ArcRwSignal;
use web_sys::HtmlVideoElement;

use super::player::{Player, PlayerState};

const LINGER_MS: u32 = 4_000;

/// Handle a widget holds while it shows a player. `Copy + Send`, so it can
/// be released from `on_cleanup`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lease(u64);

#[derive(Default)]
struct Pool {
    next: u64,
    active: HashMap<Lease, (String, Player)>,
    /// Released players, waiting to be reused or dropped.
    idle: HashMap<String, Vec<(Lease, Player, Timeout)>>,
}

thread_local! {
    static POOL: RefCell<Pool> = RefCell::default();
}

/// What a widget gets to show a player.
pub struct Acquired {
    pub lease: Lease,
    pub video: HtmlVideoElement,
    pub state: ArcRwSignal<PlayerState>,
    pub audio: ArcRwSignal<bool>,
}

/// Take an idle player for `url`, or start a new one.
pub fn acquire(url: &str) -> Option<Acquired> {
    POOL.with_borrow_mut(|pool| {
        let player = match pool.idle.get_mut(url).and_then(Vec::pop) {
            Some((_, player, _timer)) => player, // dropping the timer cancels it
            None => Player::new(url.to_string())?,
        };
        pool.next += 1;
        let lease = Lease(pool.next);
        let out = Acquired { lease, video: player.video.clone(), state: player.state.clone(), audio: player.audio.clone() };
        pool.active.insert(lease, (url.to_string(), player));
        Some(out)
    })
}

/// The element is on the page (again): make sure it plays near live.
pub fn attached(lease: Lease) {
    POOL.with_borrow(|pool| {
        if let Some((_, player)) = pool.active.get(&lease) {
            player.resume();
        }
    });
}

/// The widget is gone: detach its video and keep the player briefly.
pub fn release(lease: Lease) {
    POOL.with_borrow_mut(|pool| {
        let Some((url, player)) = pool.active.remove(&lease) else { return };
        player.video.remove();
        let key = url.clone();
        let timer = Timeout::new(LINGER_MS, move || {
            // Dropping the player closes its connection. Take it out first so
            // the drop doesn't run while the pool is borrowed.
            let expired = POOL.with_borrow_mut(|pool| {
                let list = pool.idle.get_mut(&key)?;
                let i = list.iter().position(|(l, _, _)| *l == lease)?;
                Some(list.swap_remove(i))
            });
            drop(expired.map(|(_, player, _)| player));
        });
        pool.idle.entry(url).or_default().push((lease, player, timer));
    });
}
