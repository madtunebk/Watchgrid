//! Live View navigation between three states:
//!
//! - NORMAL: the wall inside the page;
//! - GRID_FS: the wall in browser fullscreen;
//! - CAM_FS: one camera as an overlay on the wall (from either of the above).
//!
//! Only the wall ever owns browser fullscreen; a camera never requests its
//! own, so leaving the camera can't throw the wall out of fullscreen. The
//! grid stays mounted under the overlay, keeping its layout and streams.
//!
//! Opening a camera pushes a history entry, so browser Back, the mouse Back
//! button and Alt+Left end in a `popstate` that removes only the overlay.
//! Watchgrid's own close control does *not* navigate history (Chrome leaves
//! fullscreen on history navigation): it closes the overlay and turns the
//! entry into a plain grid entry.
//!
//! Escape: in fullscreen the browser consumes it to leave fullscreen. Where
//! the Keyboard Lock API exists (secure contexts: HTTPS or localhost) the
//! wall locks Escape while fullscreen, so a short press closes the camera
//! first and the grid second; holding Escape still leaves fullscreen (a
//! browser guarantee). `fullscreenchange` keeps the state honest whenever
//! the browser leaves fullscreen by itself.

use leptos::ev;
use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

/// Marks our history entries (the value is the camera id).
const STATE_KEY: &str = "watchgridLiveCamera";

#[derive(Clone, Copy)]
pub struct WallView {
    /// Camera shown as the overlay (CAM_FS), if any.
    pub focused: RwSignal<Option<String>>,
    /// The wall is the browser's fullscreen element (GRID_FS).
    pub fullscreen: RwSignal<bool>,
    wall: NodeRef<Div>,
    /// The overlay was opened while the wall was fullscreen.
    opened_in_fullscreen: StoredValue<bool>,
    /// Fullscreen was entered just for this camera: leaving it leaves fullscreen.
    exit_fullscreen_on_close: StoredValue<bool>,
    /// A history entry belongs to the open overlay.
    pushed: StoredValue<bool>,
}

impl WallView {
    /// Create the state and listen to `popstate` / `fullscreenchange` while
    /// the page lives.
    pub fn new(wall: NodeRef<Div>) -> Self {
        let view = Self {
            focused: RwSignal::new(None),
            fullscreen: RwSignal::new(false),
            wall,
            opened_in_fullscreen: StoredValue::new(false),
            exit_fullscreen_on_close: StoredValue::new(false),
            pushed: StoredValue::new(false),
        };
        let pop = window_event_listener(ev::popstate, move |e| view.on_popstate(&e.state()));
        // Fired on the fullscreen element and bubbles up to the window.
        let fs = window_event_listener_untyped("fullscreenchange", move |_| view.on_fullscreen_change());
        on_cleanup(move || {
            pop.remove();
            fs.remove();
        });
        view
    }

    /// Show one camera. `enter_fullscreen`: also make the wall fullscreen
    /// (when it isn't yet); leaving the camera then leaves fullscreen too.
    pub fn open(&self, camera_id: String, enter_fullscreen: bool) {
        let already_fs = self.fullscreen.get_untracked();
        if enter_fullscreen && !already_fs {
            if let Some(el) = self.wall.get_untracked()
                && el.request_fullscreen().is_ok()
            {
                self.exit_fullscreen_on_close.set_value(true);
            }
        }
        if self.focused.get_untracked().is_none() {
            self.opened_in_fullscreen.set_value(already_fs || enter_fullscreen);
            push_state(&camera_id);
            self.pushed.set_value(true);
        } else if self.pushed.get_value() {
            replace_state(&camera_id);
        }
        self.focused.set(Some(camera_id));
    }

    /// Leave the camera (close button, Escape). Never navigates history.
    pub fn close(&self) {
        if self.focused.get_untracked().is_none() {
            return;
        }
        if self.pushed.get_value() && current_is_ours() {
            mark_grid_entry();
        }
        self.clear();
    }

    /// Escape pressed while the page gets it (outside fullscreen, or in
    /// fullscreen with Escape locked): camera first, then the grid.
    pub fn escape(&self) {
        if self.focused.get_untracked().is_some() {
            self.close();
        } else if self.fullscreen.get_untracked() {
            document().exit_fullscreen();
        }
    }

    /// Fullscreen button for the wall itself.
    pub fn toggle_wall_fullscreen(&self) {
        self.exit_fullscreen_on_close.set_value(false);
        if document().fullscreen_element().is_some() {
            document().exit_fullscreen();
        } else if let Some(el) = self.wall.get_untracked() {
            let _ = el.request_fullscreen();
        }
    }

    fn clear(&self) {
        self.pushed.set_value(false);
        self.opened_in_fullscreen.set_value(false);
        self.focused.set(None);
        if self.exit_fullscreen_on_close.get_value() {
            self.exit_fullscreen_on_close.set_value(false);
            if document().fullscreen_element().is_some() {
                document().exit_fullscreen();
            }
        }
    }

    fn on_popstate(&self, state: &JsValue) {
        // Back to the entry below ours (or anything that isn't a camera):
        // only the overlay goes away; fullscreen is untouched.
        if self.focused.get_untracked().is_some() && !is_ours(state) {
            self.clear();
        }
    }

    fn on_fullscreen_change(&self) {
        let now = match (document().fullscreen_element(), self.wall.get_untracked()) {
            (Some(fs), Some(wall)) => {
                let wall: &web_sys::Element = &wall;
                fs == *wall
            }
            _ => false,
        };
        self.fullscreen.set(now);
        keyboard_lock_escape(now);
        if !now {
            // Fullscreen ended (possibly by the browser, e.g. Escape): never
            // keep a fullscreen-only overlay around without fullscreen.
            self.exit_fullscreen_on_close.set_value(false);
            if self.focused.get_untracked().is_some() && self.opened_in_fullscreen.get_value() {
                self.close();
            }
        }
    }
}

/// Lock (or release) the Escape key while the wall is fullscreen, where the
/// Keyboard Lock API is available. Called through JS reflection because the
/// API is still experimental in web-sys.
fn keyboard_lock_escape(lock: bool) {
    let Ok(keyboard) = js_sys::Reflect::get(&window().navigator(), &JsValue::from_str("keyboard")) else { return };
    if keyboard.is_undefined() || keyboard.is_null() {
        return; // insecure context (plain HTTP) or unsupported browser
    }
    let method = if lock { "lock" } else { "unlock" };
    let Ok(f) = js_sys::Reflect::get(&keyboard, &JsValue::from_str(method)) else { return };
    let Some(f) = wasm_bindgen::JsCast::dyn_ref::<js_sys::Function>(&f) else { return };
    let result = if lock {
        f.call1(&keyboard, &js_sys::Array::of1(&JsValue::from_str("Escape")))
    } else {
        f.call0(&keyboard)
    };
    // `lock` returns a promise; a rejection (e.g. not allowed) just means
    // Escape keeps its default browser behaviour.
    if let Ok(p) = result
        && let Ok(p) = p.dyn_into::<js_sys::Promise>()
    {
        IGNORE.with(|ignore| {
            let _ = p.catch(ignore);
        });
    }
}

thread_local! {
    static IGNORE: wasm_bindgen::closure::Closure<dyn FnMut(JsValue)> = wasm_bindgen::closure::Closure::new(|_| {});
}

/// The current entry no longer shows a camera (kept, but inert on Back).
fn mark_grid_entry() {
    if let Ok(history) = window().history() {
        let _ = history.replace_state(&JsValue::NULL, "");
    }
}

fn is_ours(state: &JsValue) -> bool {
    state.is_object() && js_sys::Reflect::get(state, &JsValue::from_str(STATE_KEY)).is_ok_and(|v| v.is_string())
}

fn current_is_ours() -> bool {
    window().history().ok().and_then(|h| h.state().ok()).is_some_and(|s| is_ours(&s))
}

fn state_for(camera_id: &str) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &JsValue::from_str(STATE_KEY), &JsValue::from_str(camera_id));
    obj.into()
}

/// Same URL, new entry: Back returns to the grid without leaving the page.
fn push_state(camera_id: &str) {
    if let Ok(history) = window().history() {
        let _ = history.push_state(&state_for(camera_id), "");
    }
}

fn replace_state(camera_id: &str) {
    if let Ok(history) = window().history() {
        let _ = history.replace_state(&state_for(camera_id), "");
    }
}
