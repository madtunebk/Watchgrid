//! `GET /api/v1/cameras/{id}/live?stream=main|sub` — live video WebSocket.
//!
//! Protocol (server → browser):
//! - text `{"type":"status","state":"connecting"|"offline","reason"?}`
//! - text `{"type":"init","codec","width","height","audio"}`, then one
//!   binary message with the fMP4 init segment (again whenever parameters
//!   change). `codec` lists every track (`avc1.…,opus`); audio is only
//!   included when the browser asked for it (`?audio=1`) and the camera has it.
//! - binary messages: fMP4 media segments, starting at a keyframe
//!
//! The browser sends nothing; closing the socket leaves the feed.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use super::fragmenter::Fragmenter;
use super::{FeedState, StreamKind, TrackInfo, fmp4};
use crate::cameras;
use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LiveQuery {
    #[serde(default)]
    stream: Option<String>,
    /// `1` when the browser can play Opus in MP4.
    #[serde(default)]
    audio: Option<String>,
}

pub async fn upgrade(ws: WebSocketUpgrade, State(state): State<AppState>, Path(id): Path<String>, Query(q): Query<LiveQuery>) -> ApiResult<Response> {
    cameras::get_camera(&state, &id).await?; // 404 for unknown cameras
    let kind = match q.stream.as_deref() {
        Some("sub") => StreamKind::Sub,
        _ => StreamKind::Main,
    };
    let wants_audio = q.audio.as_deref() == Some("1");
    Ok(ws.on_upgrade(move |socket| serve(socket, state, id, kind, wants_audio)))
}

async fn serve(mut socket: WebSocket, state: AppState, id: String, kind: StreamKind, wants_audio: bool) {
    let mut sub = state.media.subscribe(&id, kind);
    let mut fragmenter = Fragmenter::default();
    let mut current: Option<Arc<TrackInfo>> = None;
    sub.state.mark_changed();
    // Audio is sent while the current init segment has an audio track.
    let with_audio = |c: &Option<Arc<TrackInfo>>| wants_audio && c.as_ref().is_some_and(|i| i.audio.is_some());

    loop {
        let sent = tokio::select! {
            changed = sub.state.changed() => {
                if changed.is_err() {
                    return; // feed gone
                }
                let feed_state = sub.state.borrow_and_update().clone();
                on_state(&mut socket, feed_state, &mut current, &mut fragmenter, wants_audio).await
            }
            packet = sub.audio.recv(), if with_audio(&current) => match packet {
                Ok(packet) => match fragmenter.push_audio(&packet) {
                    Some(segment) => socket.send(Message::Binary(segment.into())).await,
                    None => Ok(()),
                },
                // A missed packet is a 20 ms gap; carry on.
                Err(RecvError::Lagged(_)) => Ok(()),
                Err(RecvError::Closed) => return,
            },
            frame = sub.frames.recv(), if current.is_some() => match frame {
                Ok(frame) => match fragmenter.push(frame) {
                    Some(segment) => socket.send(Message::Binary(segment.into())).await,
                    None => Ok(()),
                },
                // This viewer fell behind: skip ahead to the next keyframe.
                Err(RecvError::Lagged(_)) => {
                    fragmenter.resync();
                    Ok(())
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => Ok(()),
            }
        };
        if sent.is_err() {
            return;
        }
    }
}

async fn on_state(socket: &mut WebSocket, feed: FeedState, current: &mut Option<Arc<TrackInfo>>, fragmenter: &mut Fragmenter, wants_audio: bool) -> Result<(), axum::Error> {
    match feed {
        FeedState::Streaming(info) if !info.can_mux() => {
            *current = None;
            fragmenter.resync();
            let reason = format!("live view supports H.264 and HEVC (camera sends {})", info.codec);
            socket.send(Message::Text(json!({ "type": "status", "state": "offline", "reason": reason }).to_string().into())).await
        }
        FeedState::Streaming(info) => {
            if current.as_deref() == Some(&*info) {
                return Ok(());
            }
            let audio = info.audio.as_ref().filter(|_| wants_audio);
            let codec = if audio.is_some() { format!("{},opus", info.codec) } else { info.codec.clone() };
            let msg = json!({ "type": "init", "codec": codec, "width": info.track.width, "height": info.track.height, "audio": audio.is_some() });
            socket.send(Message::Text(msg.to_string().into())).await?;
            socket.send(Message::Binary(fmp4::init_segment(&info.track, audio).into())).await?;
            // New parameters take effect at the next keyframe.
            fragmenter.resync();
            *current = Some(info);
            Ok(())
        }
        FeedState::Connecting => {
            fragmenter.resync();
            socket.send(Message::Text(json!({ "type": "status", "state": "connecting" }).to_string().into())).await
        }
        FeedState::Failed(reason) => {
            fragmenter.resync();
            socket.send(Message::Text(json!({ "type": "status", "state": "offline", "reason": reason }).to_string().into())).await
        }
    }
}
