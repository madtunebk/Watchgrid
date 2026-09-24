//! Per-browser UI preferences in localStorage (sidebar state, view modes...).
//! Never used for real data: that belongs to the backend.

fn store() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn get(key: &str) -> Option<String> {
    store()?.get_item(key).ok().flatten()
}

pub fn set(key: &str, value: &str) {
    if let Some(s) = store() {
        let _ = s.set_item(key, value);
    }
}

pub fn get_bool(key: &str, default: bool) -> bool {
    get(key).map(|v| v == "true").unwrap_or(default)
}
