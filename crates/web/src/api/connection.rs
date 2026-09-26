//! State of the browser's link to the NVR: the server WebSocket with the
//! `live-api` feature, otherwise (mock) the browser's own online status.

#[cfg(not(feature = "live-api"))]
use std::time::Duration;

#[cfg(not(feature = "live-api"))]
use leptos::ev;
use leptos::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected,
}

#[derive(Clone, Copy)]
struct Connection(ReadSignal<ConnectionState>);

#[cfg(feature = "live-api")]
pub fn provide_connection() {
    let (state, set_state) = signal(ConnectionState::Connecting);
    super::push::start(set_state);
    provide_context(Connection(state));
}

#[cfg(not(feature = "live-api"))]
pub fn provide_connection() {
    let (state, set_state) = signal(ConnectionState::Connected);
    let offline = window_event_listener(ev::offline, move |_| set_state.set(ConnectionState::Disconnected));
    let online = window_event_listener(ev::online, move |_| {
        set_state.set(ConnectionState::Connecting);
        set_timeout(move || set_state.set(ConnectionState::Connected), Duration::from_millis(1200));
    });
    on_cleanup(move || {
        offline.remove();
        online.remove();
    });
    provide_context(Connection(state));
}

pub fn use_connection() -> ReadSignal<ConnectionState> {
    expect_context::<Connection>().0
}
