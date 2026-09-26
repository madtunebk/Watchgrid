//! Browser fullscreen for any element (video tiles, grids).

use leptos::prelude::document;

/// Enter fullscreen on `el`, or leave fullscreen if something is already fullscreen.
pub fn toggle(el: &web_sys::Element) {
    let doc = document();
    if doc.fullscreen_element().is_some() {
        doc.exit_fullscreen();
    } else {
        let _ = el.request_fullscreen();
    }
}
