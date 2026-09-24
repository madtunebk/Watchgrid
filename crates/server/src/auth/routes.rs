//! `/api/v1/auth` endpoints. Sign-in only — no user creation, deletion or
//! password reset over HTTP.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use watchgrid_model::{Role, Session, User};
use zeroize::Zeroizing;

use super::guard::{CurrentUser, cookie_token, session_lifetime};
use super::{COOKIE, password, sessions, users};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/session", get(current))
        .route("/users", get(list_users))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", delete(revoke))
}

/// Set `WATCHGRID_SECURE_COOKIES=1` when Watchgrid is served over HTTPS
/// (directly or behind a TLS proxy).
fn secure_cookies() -> bool {
    std::env::var("WATCHGRID_SECURE_COOKIES").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn session_cookie(token: &str, max_age_secs: i64) -> HeaderValue {
    let secure = if secure_cookies() { "; Secure" } else { "" };
    let value = format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_secs}{secure}");
    HeaderValue::from_str(&value).expect("cookie is ASCII")
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: Zeroizing<String>,
}

async fn no_users(state: &AppState) -> ApiResult<bool> {
    Ok(users::count(&state.db).await? == 0)
}

async fn login(
    State(s): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> ApiResult<Response> {
    let ip = peer.map_or(IpAddr::V4(Ipv4Addr::LOCALHOST), |Extension(ConnectInfo(a))| a.ip());
    if no_users(&s).await? {
        return Err(ApiError::no_users());
    }
    if s.login_limiter.blocked(ip) {
        return Err(ApiError::too_many("Too many failed sign-ins. Try again in a few minutes."));
    }
    let username = c.username.trim().to_lowercase();
    let user = users::find(&s.db, &username).await?;
    let ok = match &user {
        Some(u) => password::verify(&c.password, &u.password_hash) && u.enabled,
        None => {
            password::verify_dummy(&c.password);
            false
        }
    };
    let Some(user) = user.filter(|_| ok) else {
        s.login_limiter.failed(ip);
        tracing::warn!(%ip, username = %username, "failed sign-in");
        return Err(ApiError::invalid_credentials());
    };
    s.login_limiter.succeeded(ip);

    // A fresh token on every sign-in (session rotation); an old cookie
    // from this browser is ended.
    let old = cookie_token_from(&headers);
    if let Some(old) = old
        && let Ok(Some(a)) = sessions::find(&s.db, &old).await
    {
        let _ = sessions::delete(&s.db, &a.session_id).await;
    }
    let lifetime = session_lifetime(&s).await;
    let client: String = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("unknown").chars().take(200).collect();
    let (_, token) = sessions::create(&s.db, user.id, &client, &ip.to_string(), lifetime).await?;
    users::touch_login(&s.db, user.id).await?;
    let _ = sessions::purge_expired(&s.db).await;
    tracing::info!(%ip, username = %user.username, "signed in");

    let mut resp = Json(user.public()).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, session_cookie(&token, lifetime.num_seconds()));
    Ok(resp)
}

fn cookie_token_from(headers: &HeaderMap) -> Option<String> {
    let mut req = Request::new(axum::body::Body::empty());
    *req.headers_mut() = headers.clone();
    cookie_token(&req)
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Response> {
    if let Some(token) = cookie_token_from(&headers)
        && let Some(a) = sessions::find(&s.db, &token).await?
    {
        sessions::delete(&s.db, &a.session_id).await?;
        tracing::info!(username = %a.username, "signed out");
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(header::SET_COOKIE, session_cookie("", 0));
    Ok(resp)
}

/// Who am I? 401 `unauthenticated`, or 401 `no_users` when none exist yet.
async fn current(State(s): State<AppState>, headers: HeaderMap) -> ApiResult<Json<User>> {
    if no_users(&s).await? {
        return Err(ApiError::no_users());
    }
    let token = cookie_token_from(&headers).ok_or_else(ApiError::unauthenticated)?;
    let active = sessions::find(&s.db, &token).await?.ok_or_else(ApiError::unauthenticated)?;
    let user = users::find(&s.db, &active.username).await?.ok_or_else(ApiError::unauthenticated)?;
    Ok(Json(user.public()))
}

fn require_admin(me: &CurrentUser) -> ApiResult<()> {
    if me.role == Role::Admin { Ok(()) } else { Err(ApiError::forbidden("Only administrators can see this")) }
}

/// Read-only list for administrators.
async fn list_users(State(s): State<AppState>, Extension(me): Extension<CurrentUser>) -> ApiResult<Json<Vec<User>>> {
    require_admin(&me)?;
    Ok(Json(users::all(&s.db).await?.iter().map(|u| u.public()).collect()))
}

/// Administrators see every session; viewers their own.
async fn list_sessions(State(s): State<AppState>, Extension(me): Extension<CurrentUser>) -> ApiResult<Json<Vec<Session>>> {
    let scope = (me.role != Role::Admin).then_some(me.user_id);
    let rows = sessions::list(&s.db, scope).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, username, client, address, last_seen)| Session { current: id == me.session_id, id, username, client, address, last_seen })
            .collect(),
    ))
}

async fn revoke(State(s): State<AppState>, Extension(me): Extension<CurrentUser>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let owner = sessions::owner(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Session"))?;
    if owner != me.user_id {
        require_admin(&me)?;
    }
    sessions::delete(&s.db, &id).await?;
    tracing::info!(by = %me.username, session = %id, "session revoked");
    Ok(StatusCode::NO_CONTENT)
}
