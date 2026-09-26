//! Sign-in and session management over HTTP. Accounts themselves are
//! managed only from the server CLI.

use super::client;
use crate::api::{ApiResult, Session, User};

pub async fn current() -> ApiResult<User> {
    client::get("/auth/session").await
}

pub async fn login(username: &str, password: &str) -> ApiResult<User> {
    client::post("/auth/login", Some(&serde_json::json!({ "username": username, "password": password }))).await
}

pub async fn logout() -> ApiResult<()> {
    client::post_no_content("/auth/logout").await
}

pub async fn users() -> ApiResult<Vec<User>> {
    client::get("/auth/users").await
}

pub async fn sessions() -> ApiResult<Vec<Session>> {
    client::get("/auth/sessions").await
}

pub async fn revoke(id: &str) -> ApiResult<()> {
    client::delete(&format!("/auth/sessions/{}", String::from(js_sys::encode_uri_component(id)))).await
}
