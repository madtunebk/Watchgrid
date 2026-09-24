//! HTTP surface: `/api/v1/*` plus the built web UI (single-page app).

use std::path::Path;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

use crate::cameras;
use crate::error::ApiError;
use crate::media;
use crate::recordings;
use crate::storage;
use crate::state::AppState;
use crate::ws;

pub fn router(state: AppState, ui_dir: &Path) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws::upgrade))
        .nest("/cameras", cameras::router().route("/{id}/live", get(media::upgrade)))
        .nest("/recordings", recordings::router())
        .nest("/storage", storage::router())
        .fallback(|| async { ApiError::not_found("API endpoint") });

    // Unknown non-API paths are client-side routes: serve the app shell.
    let ui = ServeDir::new(ui_dir).fallback(ServeFile::new(ui_dir.join("index.html")));

    Router::new().nest("/api/v1", api).fallback_service(ui).with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
