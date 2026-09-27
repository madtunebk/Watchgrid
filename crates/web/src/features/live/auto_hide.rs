//! Controls floating over the wall that step aside when nobody uses them:
//! shown on any mouse, touch or key activity, hidden after a few quiet
//! seconds, never while the pointer rests on them or one has focus.

use std::time::Duration;

use leptos::prelude::*;

const QUIET: Duration = Duration::from_secs(3);

#[derive(Clone, Copy)]
pub struct AutoHide {
    pub shown: RwSignal<bool>,
    held: RwSignal<bool>,
    /// Only the newest timer may hide.
    token: StoredValue<u64>,
}

impl AutoHide {
    /// Starts shown.
    pub fn new() -> Self {
        let this = Self { shown: RwSignal::new(true), held: RwSignal::new(false), token: StoredValue::new(0) };
        this.wake();
        this
    }

    /// Activity: show, and hide again after a quiet moment.
    pub fn wake(self) {
        self.shown.set(true);
        let Some(t) = self.token.try_update_value(|t| {
            *t += 1;
            *t
        }) else {
            return;
        };
        set_timeout(
            move || {
                if self.token.try_get_value() == Some(t) && self.held.try_get_untracked() == Some(false) {
                    self.shown.try_set(false);
                }
            },
            QUIET,
        );
    }

    /// The pointer or focus is on a control: stay shown until it leaves.
    pub fn hold(self, on: bool) {
        self.held.set(on);
        if !on {
            self.wake();
        }
    }
}
