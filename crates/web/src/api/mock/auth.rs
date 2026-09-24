//! Mock users and sessions.

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiError, ApiResult, Session, User};

pub async fn users() -> ApiResult<Vec<User>> {
    latency().await;
    Ok(with_db(|db| db.users.clone()))
}

pub async fn sessions() -> ApiResult<Vec<Session>> {
    latency().await;
    Ok(with_db(|db| db.sessions.clone()))
}

pub async fn revoke(id: &str) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let s = db.sessions.iter().find(|s| s.id == id).ok_or_else(|| ApiError::not_found("Session"))?;
        if s.current {
            return Err(ApiError::conflict("Use Sign out to end your own session"));
        }
        db.sessions.retain(|s| s.id != id);
        Ok(())
    })
}

/// The mock UI runs as a signed-in administrator.
pub async fn current() -> ApiResult<User> {
    Ok(User { id: "1".into(), username: "admin".into(), role: crate::api::Role::Admin, last_login: None })
}

pub async fn login(_username: &str, _password: &str) -> ApiResult<User> {
    current().await
}

pub async fn logout() -> ApiResult<()> {
    Ok(())
}
