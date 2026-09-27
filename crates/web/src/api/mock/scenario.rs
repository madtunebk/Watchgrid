//! Mock scenarios selected with `?mock=<name>` in the page URL.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Realistic home setup with five cameras.
    Default,
    /// Fresh installation: no cameras, events or recordings.
    Empty,
    /// Recording volume missing / unmounted.
    NoStorage,
    /// Forty cameras, for pagination and capacity checks.
    Large,
}

/// A `name=value` from the page URL (mock switches such as `mock=` or `ptz=`).
pub fn param(name: &str) -> String {
    let search = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
    search.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('=')).unwrap_or("").to_string()
}

pub fn current() -> Scenario {
    match param("mock").as_str() {
        "empty" => Scenario::Empty,
        "nostorage" => Scenario::NoStorage,
        "large" => Scenario::Large,
        _ => Scenario::Default,
    }
}
