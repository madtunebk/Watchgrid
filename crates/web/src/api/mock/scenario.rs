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

pub fn current() -> Scenario {
    let search = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
    let value = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix("mock="))
        .unwrap_or("");
    match value {
        "empty" => Scenario::Empty,
        "nostorage" => Scenario::NoStorage,
        "large" => Scenario::Large,
        _ => Scenario::Default,
    }
}
