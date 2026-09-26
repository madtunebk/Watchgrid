//! Copy text to the system clipboard.

use wasm_bindgen_futures::JsFuture;

/// Resolves to `true` when the browser accepted the text.
pub async fn copy(text: &str) -> bool {
    let Some(window) = web_sys::window() else { return false };
    JsFuture::from(window.navigator().clipboard().write_text(text)).await.is_ok()
}
