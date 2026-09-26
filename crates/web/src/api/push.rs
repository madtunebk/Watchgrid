//! Live updates from the server (`/api/v1/ws`, feature `live-api`).
//!
//! The server only says which data changed ("cameras", "system", …); the
//! UI refetches those queries. The socket's state drives the connection
//! indicator, and it reconnects with backoff.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{MessageEvent, WebSocket};

use super::connection::ConnectionState;
use super::query::{Topic, invalidate};
use super::throttle::{Action, Gate};

const MAX_DELAY: Duration = Duration::from_secs(15);

fn topic(name: &str) -> Option<Topic> {
    match name {
        "cameras" => Some(Topic::Cameras),
        "system" => Some(Topic::System),
        "storage" => Some(Topic::Storage),
        "events" => Some(Topic::Events),
        "recordings" => Some(Topic::Recordings),
        "settings" => Some(Topic::Settings),
        "notifications" => Some(Topic::Notifications),
        "server" => Some(Topic::Server),
        _ => None,
    }
}

thread_local! {
    static GATES: RefCell<HashMap<usize, Gate>> = RefCell::new(HashMap::new());
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Refresh `t`, folding bursts (see `throttle`).
fn refresh(t: Topic) {
    let action = GATES.with(|g| g.borrow_mut().entry(t as usize).or_default().notice(now_ms()));
    match action {
        Action::Now => invalidate(t),
        Action::Later(ms) => set_timeout(
            move || {
                GATES.with(|g| g.borrow_mut().entry(t as usize).or_default().fired(now_ms()));
                invalidate(t);
            },
            Duration::from_millis(ms.max(0.0) as u64),
        ),
        Action::Folded => {}
    }
}

#[derive(serde::Deserialize)]
struct Notice {
    topics: Vec<String>,
}

pub fn start(set_state: WriteSignal<ConnectionState>) {
    connect(set_state, 0);
}

fn connect(set_state: WriteSignal<ConnectionState>, attempt: u32) {
    let Some(window) = web_sys::window() else { return };
    let loc = window.location();
    let scheme = if loc.protocol().as_deref() == Ok("https:") { "wss" } else { "ws" };
    let url = format!("{scheme}://{}/api/v1/ws", loc.host().unwrap_or_default());
    if attempt > 0 {
        set_state.set(ConnectionState::Connecting);
    }
    let Ok(ws) = WebSocket::new(&url) else {
        retry(set_state, attempt);
        return;
    };

    let on_open = Closure::<dyn FnMut()>::new(move || {
        set_state.set(ConnectionState::Connected);
        // Anything may have changed while we were away.
        if attempt > 0 {
            for t in [Topic::Cameras, Topic::System, Topic::Storage] {
                invalidate(t);
            }
        }
    });
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        let Some(text) = e.data().as_string() else { return };
        if let Ok(notice) = serde_json::from_str::<Notice>(&text) {
            for t in notice.topics.iter().filter_map(|n| topic(n)) {
                refresh(t);
            }
        }
    });
    let on_close = Closure::<dyn FnMut()>::new(move || {
        set_state.set(ConnectionState::Disconnected);
        retry(set_state, attempt + 1);
    });
    ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    // The socket lives as long as the page; its handlers must too.
    on_open.forget();
    on_message.forget();
    on_close.forget();
}

fn retry(set_state: WriteSignal<ConnectionState>, attempt: u32) {
    let delay = Duration::from_millis(500 * 2u64.saturating_pow(attempt.min(5))).min(MAX_DELAY);
    set_timeout(move || connect(set_state, attempt), delay);
}
