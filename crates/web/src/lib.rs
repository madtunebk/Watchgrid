//! Watchgrid web UI.
//!
//! Client-side rendered Leptos app compiled to wasm32. All data goes through
//! [`api`], which today is an in-memory mock and later becomes the HTTP/WebSocket
//! client for the Rust backend. Components never touch mock data directly.

// Scaffolding used by later milestone steps; remove in step 10 (architecture review).
#![allow(dead_code)]

mod api;
mod app;
mod clock;
mod features;
mod format;
#[cfg(feature = "live-api")]
mod live_video;
mod prefs;
mod shell;
mod ui;

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
