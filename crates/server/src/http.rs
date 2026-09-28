//! HTTP surface: `/api/v1/*` plus the built web UI (single-page app).

use std::path::Path;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

use crate::{auth, cameras};
use crate::error::ApiError;
use crate::events;
use crate::exports;
use crate::media;
use crate::notifications;
use crate::recordings;
use crate::storage;
use crate::settings;
use crate::system;
use crate::state::AppState;
use crate::ws;

pub fn router(state: AppState, ui_dir: &Path) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws::upgrade))
        .nest("/arm", crate::arm::router())
        .nest("/cameras", cameras::router().route("/{id}/live", get(media::upgrade)))
        .nest("/events", events::router().route("/{id}/export", axum::routing::post(exports::export_event)))
        .nest("/exports", exports::router())
        .nest("/notifications", notifications::router())
        .nest("/recordings", recordings::router().route("/{id}/export", axum::routing::post(exports::export_recording)))
        .nest("/storage", storage::router())
        .nest("/system", system::router())
        .nest("/settings", settings::router())
        .nest("/auth", auth::router())
        .fallback(|| async { ApiError::not_found("API endpoint") })
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::require_session));

    // Unknown non-API paths are client-side routes: serve the app shell.
    let ui = Router::new()
        .fallback_service(ServeDir::new(ui_dir).fallback(ServeFile::new(ui_dir.join("index.html"))))
        .layer(axum::middleware::from_fn(ui_caching));

    Router::new().nest("/api/v1", api).fallback_service(ui).with_state(state)
}

/// After an update the browser must load the new app without a hard
/// refresh. The page (`index.html`, every client route) is checked each
/// time; the files it loads carry the build in `?v=`, so they can be kept
/// for good — a new build asks for new URLs.
async fn ui_caching(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    use axum::http::{HeaderValue, header};
    let versioned = req.uri().query().is_some_and(|q| q.split('&').any(|p| p.starts_with("v=")));
    let mut res = next.run(req).await;
    let html = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.starts_with("text/html"));
    let rule = if html {
        "no-cache"
    } else if versioned && res.status().is_success() {
        "public, max-age=31536000, immutable"
    } else {
        return res;
    };
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(rule));
    res
}


async fn health() -> Json<Value> {
    // `build`: the git commit, set by the release packaging (else "dev").
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION"), "build": option_env!("WATCHGRID_BUILD").unwrap_or("dev") }))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn the_page_is_checked_each_time_and_versioned_files_are_kept() {
        let dir = std::env::temp_dir().join(format!("watchgrid-ui-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(dir.join("app.js"), "x").unwrap();
        let ui = Router::new()
            .fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(dir.join("index.html"))))
            .layer(axum::middleware::from_fn(ui_caching));
        let cache = |uri: &'static str| {
            let ui = ui.clone();
            async move {
                let res = ui.oneshot(Request::get(uri).body(Body::empty()).unwrap()).await.unwrap();
                res.headers().get(header::CACHE_CONTROL).map(|v| v.to_str().unwrap().to_string())
            }
        };
        assert_eq!(cache("/").await.as_deref(), Some("no-cache"));
        assert_eq!(cache("/cameras/cam-1").await.as_deref(), Some("no-cache"), "client routes get the page too");
        assert_eq!(cache("/app.js?v=abc123").await.as_deref(), Some("public, max-age=31536000, immutable"));
        assert_eq!(cache("/app.js").await, None, "not versioned: browser's own rules");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
