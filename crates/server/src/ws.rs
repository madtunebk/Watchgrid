//! `GET /api/v1/ws` — pushes "something changed" notices to the web UI so
//! it refreshes the affected data. Only topics are sent, never data or
//! secrets; the UI refetches through the normal (authorized) API.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use crate::bus::BusEvent;
use crate::state::AppState;

pub async fn upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| serve(socket, state))
}

/// UI query topics affected by an event.
fn topics(event: &BusEvent) -> &'static [&'static str] {
    match event {
        BusEvent::CameraOnline { .. } | BusEvent::CameraOffline { .. } | BusEvent::CameraStopped { .. } => &["cameras", "system"],
        BusEvent::EventsChanged => &["events"],
        BusEvent::NotificationsChanged => &["notifications"],
        BusEvent::DetectionStarted { .. } | BusEvent::DetectionEnded { .. } => &["cameras"],
        BusEvent::SettingsChanged => &["settings", "server"],
        BusEvent::CamerasChanged => &["cameras", "system", "storage"],
        BusEvent::StorageChanged => &["storage"],
        BusEvent::RecordingsChanged => &["recordings", "storage", "cameras"],
        BusEvent::RecordingStarted { .. } => &["cameras"],
        BusEvent::RecordingStopped { .. } => &["cameras", "recordings", "storage"],
    }
}

async fn serve(mut socket: WebSocket, state: AppState) {
    let mut events = state.bus.subscribe();
    loop {
        tokio::select! {
            event = events.recv() => {
                let names = match event {
                    Ok(e) => topics(&e),
                    // Missed some events: refresh everything the UI shows.
                    Err(RecvError::Lagged(_)) => &["cameras", "system", "storage", "recordings", "events"][..],
                    Err(RecvError::Closed) => return,
                };
                let msg = format!(r#"{{"topics":["{}"]}}"#, names.join("\",\""));
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => match incoming {
                // The UI never sends commands here; close or error ends the session.
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_refresh_cameras() {
        assert!(topics(&BusEvent::CameraOnline { camera_id: "x".into(), at: chrono::Utc::now() }).contains(&"cameras"));
        assert!(topics(&BusEvent::CamerasChanged).contains(&"storage"));
    }
}
