//! Sign-in flow through the real router (`sqlx::test`).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use sqlx::PgPool;
use tower::ServiceExt;
use watchgrid_model::Role;

use super::{password, users};
use crate::credentials::CredentialStore;
use crate::state::AppState;

fn app(db: PgPool) -> Router {
    crate::http::router(AppState::inert(db, CredentialStore::from_key(&[3u8; 32])), std::path::Path::new("dist"))
}

async fn call(app: &Router, method: &str, path: &str, cookie: Option<&str>, body: Option<&str>) -> (StatusCode, Option<String>, String) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    if body.is_some() {
        req = req.header(header::CONTENT_TYPE, "application/json");
    }
    let resp = app.clone().oneshot(req.body(Body::from(body.unwrap_or("").to_string())).unwrap()).await.unwrap();
    let status = resp.status();
    let set_cookie = resp.headers().get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_string());
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (status, set_cookie, String::from_utf8_lossy(&bytes).into_owned())
}

async fn add_user(db: &PgPool, name: &str, role: Role) {
    users::create(db, name, &password::hash("correct horse battery").unwrap(), role).await.unwrap();
}

async fn sign_in(app: &Router, name: &str) -> String {
    let body = format!(r#"{{"username":"{name}","password":"correct horse battery"}}"#);
    let (status, cookie, _) = call(app, "POST", "/api/v1/auth/login", None, Some(&body)).await;
    assert_eq!(status, StatusCode::OK);
    let cookie = cookie.expect("session cookie");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    cookie.split(';').next().unwrap().to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn api_needs_a_session_and_reports_missing_users(db: PgPool) {
    let app = app(db.clone());
    assert_eq!(call(&app, "GET", "/api/v1/health", None, None).await.0, StatusCode::OK, "health is public");
    let (status, _, body) = call(&app, "GET", "/api/v1/cameras", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("unauthenticated"));
    let (status, _, body) = call(&app, "GET", "/api/v1/auth/session", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("no_users") && body.contains("watchgrid user create"));
}

#[sqlx::test(migrations = "./migrations")]
async fn sign_in_use_and_sign_out(db: PgPool) {
    let app = app(db.clone());
    add_user(&db, "alice", Role::Admin).await;

    let wrong = r#"{"username":"alice","password":"nope"}"#;
    let (status, cookie, body) = call(&app, "POST", "/api/v1/auth/login", None, Some(wrong)).await;
    assert_eq!((status, cookie), (StatusCode::UNAUTHORIZED, None));
    assert!(body.contains("invalid_credentials"));
    let unknown = r#"{"username":"mallory","password":"correct horse battery"}"#;
    assert!(call(&app, "POST", "/api/v1/auth/login", None, Some(unknown)).await.2.contains("invalid_credentials"), "same answer for unknown users");

    let cookie = sign_in(&app, "alice").await;
    assert_eq!(call(&app, "GET", "/api/v1/cameras", Some(&cookie), None).await.0, StatusCode::OK);
    let (status, _, me) = call(&app, "GET", "/api/v1/auth/session", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(me.contains(r#""username":"alice""#) && !me.contains("argon2"), "never the hash");
    let (_, _, list) = call(&app, "GET", "/api/v1/auth/sessions", Some(&cookie), None).await;
    assert!(list.contains(r#""current":true"#));

    let (status, cleared, _) = call(&app, "POST", "/api/v1/auth/logout", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(cleared.unwrap().contains("Max-Age=0"));
    assert_eq!(call(&app, "GET", "/api/v1/cameras", Some(&cookie), None).await.0, StatusCode::UNAUTHORIZED, "old cookie is dead");
}

#[sqlx::test(migrations = "./migrations")]
async fn viewers_are_read_only_and_disabled_users_cannot_sign_in(db: PgPool) {
    let app = app(db.clone());
    add_user(&db, "vera", Role::Viewer).await;
    let cookie = sign_in(&app, "vera").await;
    assert_eq!(call(&app, "GET", "/api/v1/cameras", Some(&cookie), None).await.0, StatusCode::OK);
    assert_eq!(call(&app, "PUT", "/api/v1/storage/retention", Some(&cookie), Some("{}")).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "GET", "/api/v1/auth/users", Some(&cookie), None).await.0, StatusCode::FORBIDDEN);

    users::set_enabled(&db, "vera", false).await.unwrap();
    assert_eq!(call(&app, "GET", "/api/v1/cameras", Some(&cookie), None).await.0, StatusCode::UNAUTHORIZED, "disabling ends access");
    let body = r#"{"username":"vera","password":"correct horse battery"}"#;
    assert_eq!(call(&app, "POST", "/api/v1/auth/login", None, Some(body)).await.0, StatusCode::UNAUTHORIZED);
}

#[test]
fn usernames_are_restricted() {
    assert!(users::valid_username("alice") && users::valid_username("ops.team-2"));
    assert!(!users::valid_username("Alice") && !users::valid_username("") && !users::valid_username("a b") && !users::valid_username(&"x".repeat(33)));
}
