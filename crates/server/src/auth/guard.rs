//! Request guard: every API call needs a valid session, except a few
//! public endpoints; viewers may only read.

use axum::extract::{Request, State};
use axum::http::{Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Duration;
use watchgrid_model::Role;

use super::{COOKIE, sessions};
use crate::error::ApiError;
use crate::state::AppState;

/// The signed-in user, available to handlers as an extension.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub session_id: String,
    pub user_id: i64,
    pub username: String,
    pub role: Role,
}

/// Paths (under `/api/v1`) reachable without a session.
const PUBLIC: [&str; 4] = ["/health", "/auth/login", "/auth/logout", "/auth/session"];

/// Writes a viewer may still do (their own sign-out and sessions).
fn viewer_may_write(path: &str) -> bool {
    path == "/auth/logout" || path.starts_with("/auth/sessions/")
}

pub fn cookie_token(req: &Request) -> Option<String> {
    req.headers().get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == COOKIE && !v.is_empty()).then(|| v.to_string())
    })
}

pub async fn session_lifetime(state: &AppState) -> Duration {
    let minutes = crate::settings::load_app(&state.db, state.bind).await.map(|s| s.auth.session_timeout_minutes).unwrap_or(720);
    Duration::minutes(i64::from(minutes))
}

pub async fn require_session(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    // Mounted under /api/v1, so the path here is relative to it.
    let path = req.uri().path().to_string();
    if PUBLIC.contains(&path.as_str()) {
        return next.run(req).await;
    }
    let Some(token) = cookie_token(&req) else {
        return ApiError::unauthenticated().into_response();
    };
    let active = match sessions::find(&state.db, &token).await {
        Ok(Some(a)) => a,
        Ok(None) => return ApiError::unauthenticated().into_response(),
        Err(e) => return ApiError::internal(e).into_response(),
    };
    let writes = !matches!(*req.method(), Method::GET | Method::HEAD);
    if writes && active.role != Role::Admin && !viewer_may_write(&path) {
        return ApiError::forbidden("Viewers can't change anything").into_response();
    }
    // Sliding expiry, written at most once a minute.
    if (chrono::Utc::now() - active.last_seen).num_seconds() >= 60 {
        let lifetime = session_lifetime(&state).await;
        if let Err(e) = sessions::touch(&state.db, &active.session_id, lifetime).await {
            tracing::warn!("cannot refresh session: {e}");
        }
    }
    req.extensions_mut().insert(CurrentUser { session_id: active.session_id, user_id: active.user_id, username: active.username, role: active.role });
    next.run(req).await
}
