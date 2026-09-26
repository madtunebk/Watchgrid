//! What a `<video>` is doing, shown over it: a spinner while it loads or
//! buffers, a plain message if it can't play. Without it a slow clip looks
//! like a broken player.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::icons::{I, Icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    Loading,
    Ready,
    /// `MediaError` code: 2 network, 3 decode, 4 not found / not playable.
    Failed(u16),
}

impl VideoState {
    /// The new state after a media event on `ev`'s `<video>`.
    pub fn after(event: &str, ev: &web_sys::Event) -> Option<Self> {
        Some(match event {
            "loadstart" | "waiting" => Self::Loading,
            "loadedmetadata" | "canplay" | "playing" => Self::Ready,
            "error" => {
                let code = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlMediaElement>().ok()).and_then(|v| v.error()).map_or(0, |e| e.code());
                Self::Failed(code)
            }
            _ => return None,
        })
    }
}

fn failure(code: u16) -> &'static str {
    match code {
        2 => "The clip stopped loading: the connection to the server dropped.",
        3 => "The clip can't be decoded by this browser.",
        _ => "The clip can't be loaded. It may have been deleted, or it isn't saved yet.",
    }
}

/// Covers the video (its parent must be positioned) while not ready.
#[component]
pub fn VideoOverlay(state: RwSignal<VideoState>) -> impl IntoView {
    move || match state.get() {
        VideoState::Ready => ().into_any(),
        VideoState::Loading => view! {
            <div class="video-state" role="status">
                <Icon icon=I::Loader class="icon icon--lg spin" />
                <span>"Loading clip…"</span>
            </div>
        }.into_any(),
        VideoState::Failed(code) => view! {
            <div class="video-state video-state--failed" role="alert">
                <Icon icon=I::TriangleAlert class="icon icon--lg" />
                <span>{failure(code)}</span>
            </div>
        }.into_any(),
    }
}
