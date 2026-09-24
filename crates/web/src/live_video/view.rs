//! `<LiveVideo>`: a camera's live stream inside a preview surface.

use leptos::html::Div;
use leptos::prelude::*;

use super::player::PlayerState;
use super::pool;
use crate::ui::{I, Icon};

#[component]
pub fn LiveVideo(camera_id: String, substream: bool) -> impl IntoView {
    let host = NodeRef::<Div>::new();
    let Some((lease, video, state)) = pool::acquire(&socket_url(&camera_id, substream)) else {
        return view! { <div class="preview__state"><span>"Live video unavailable"</span></div> }.into_any();
    };
    host.on_load(move |el| {
        let _ = el.prepend_with_node_1(&video);
        pool::attached(lease);
    });
    on_cleanup(move || pool::release(lease));

    let overlay = move || match state.get() {
        PlayerState::Playing => None,
        PlayerState::Connecting => Some(view! {
            <div class="preview__state"><Icon icon=I::Loader class="icon icon--xl spin" /><span>"Starting live view…"</span></div>
        }.into_any()),
        PlayerState::Offline(reason) => Some(view! {
            <div class="preview__state preview__state--offline"><Icon icon=I::VideoOff class="icon icon--xl" /><span>"Stream unavailable"</span><small>{reason}</small></div>
        }.into_any()),
        PlayerState::Unsupported(reason) => Some(view! {
            <div class="preview__state preview__state--offline"><Icon icon=I::VideoOff class="icon icon--xl" /><span>"Can't play this stream"</span><small>{reason}</small></div>
        }.into_any()),
    };

    view! { <div class="preview__surface" node_ref=host>{overlay}</div> }.into_any()
}

fn socket_url(camera_id: &str, substream: bool) -> String {
    let loc = web_sys::window().map(|w| w.location());
    let secure = loc.as_ref().and_then(|l| l.protocol().ok()).as_deref() == Some("https:");
    let host = loc.and_then(|l| l.host().ok()).unwrap_or_default();
    let stream = if substream { "sub" } else { "main" };
    let id = js_sys::encode_uri_component(camera_id);
    format!("{}://{host}/api/v1/cameras/{id}/live?stream={stream}", if secure { "wss" } else { "ws" })
}
