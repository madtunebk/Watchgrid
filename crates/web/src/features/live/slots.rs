//! Which camera sits in which tile, across all pages. Persisted per
//! browser, so each viewer (wall display, laptop, phone) keeps its own
//! arrangement. Tile `i` is on page `i / cells_per_page`.

use leptos::prelude::*;

use crate::api::Camera;
use crate::prefs;

const KEY: &str = "ui.live.slots";

#[derive(Clone, Copy)]
pub struct Slots(RwSignal<Vec<Option<String>>>);

impl Slots {
    /// Load the saved arrangement; the flag says whether one existed.
    pub fn load() -> (Self, bool) {
        let saved = prefs::get(KEY);
        let slots: Vec<Option<String>> = saved
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| (!s.is_empty()).then(|| s.to_string()))
            .collect();
        let slots = Self(RwSignal::new(slots));
        Effect::new(move || {
            let text = slots.0.with(|s| {
                let end = s.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
                s[..end].iter().map(|s| s.as_deref().unwrap_or("")).collect::<Vec<_>>().join(",")
            });
            prefs::set(KEY, &text);
        });
        (slots, saved.is_some())
    }

    /// Subscribe the current reactive scope to any change of the arrangement.
    pub fn track(self) {
        self.0.track();
    }

    pub fn get(self, i: usize) -> Option<String> {
        self.0.with(|s| s.get(i).cloned().flatten())
    }

    fn ensure(s: &mut Vec<Option<String>>, len: usize) {
        if s.len() < len {
            s.resize(len, None);
        }
    }

    pub fn swap(self, a: usize, b: usize) {
        if a != b {
            self.0.update(|s| {
                Self::ensure(s, a.max(b) + 1);
                s.swap(a, b);
            });
        }
    }

    pub fn assign(self, i: usize, camera_id: String) {
        self.0.update(|s| {
            // A camera appears in one tile only.
            for slot in s.iter_mut() {
                if slot.as_deref() == Some(camera_id.as_str()) {
                    *slot = None;
                }
            }
            Self::ensure(s, i + 1);
            s[i] = Some(camera_id);
        });
    }

    pub fn clear(self, i: usize) {
        self.0.update(|s| {
            if let Some(slot) = s.get_mut(i) {
                *slot = None;
            }
        });
    }

    /// Camera ids on tiles `start..start + len`.
    pub fn range(self, start: usize, len: usize) -> Vec<String> {
        self.0.with(|s| s.iter().skip(start).take(len).flatten().cloned().collect())
    }

    /// Every placed camera id, on any page.
    pub fn placed(self) -> Vec<String> {
        self.0.with(|s| s.iter().flatten().cloned().collect())
    }

    /// One past the last used tile.
    pub fn extent(self) -> usize {
        self.0.with(|s| s.iter().rposition(Option::is_some).map_or(0, |i| i + 1))
    }

    /// Put cameras in their natural order (enabled ones first).
    pub fn arrange(self, cameras: &[Camera]) {
        let mut list: Vec<&Camera> = cameras.iter().collect();
        list.sort_by_key(|c| !c.enabled);
        self.0.set(list.into_iter().map(|c| Some(c.id.clone())).collect());
    }

    /// Drop cameras that no longer exist.
    pub fn forget_missing(self, cameras: &[Camera]) {
        let exists = |id: &String| cameras.iter().any(|c| &c.id == id);
        if self.0.with_untracked(|s| s.iter().flatten().any(|id| !exists(id))) {
            self.0.update(|s| {
                for slot in s.iter_mut() {
                    if slot.as_ref().is_some_and(|id| !exists(id)) {
                        *slot = None;
                    }
                }
            });
        }
    }
}
