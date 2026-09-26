//! SQL for browser sessions. Tokens are looked up by hash only.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;
use watchgrid_model::Role;

use super::{tokens, users};

/// A valid session with its user.
#[derive(Debug, Clone)]
pub struct Active {
    pub session_id: String,
    pub user_id: i64,
    pub username: String,
    pub role: Role,
    pub last_seen: DateTime<Utc>,
}

/// Create a session; returns (public id, cookie token).
pub async fn create(db: &PgPool, user_id: i64, client: &str, address: &str, lifetime: Duration) -> sqlx::Result<(String, String)> {
    let (id, token) = (tokens::new_session_id(), tokens::new_token());
    sqlx::query("INSERT INTO sessions (id, token_hash, user_id, client, address, expires_at) VALUES ($1, $2, $3, $4, $5, now() + $6)")
        .bind(&id)
        .bind(tokens::hash(&token))
        .bind(user_id)
        .bind(client)
        .bind(address)
        .bind(lifetime)
        .execute(db)
        .await?;
    Ok((id, token))
}

/// The session for a cookie token, if it is valid and its user enabled.
pub async fn find(db: &PgPool, token: &str) -> sqlx::Result<Option<Active>> {
    let row: Option<(String, i64, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT s.id, u.id, u.username, u.role, s.last_seen FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now() AND u.enabled",
    )
    .bind(tokens::hash(token))
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(session_id, user_id, username, role, last_seen)| Active {
        session_id,
        user_id,
        username,
        role: users::parse_role(&role).unwrap_or(Role::Viewer),
        last_seen,
    }))
}

/// Sliding expiry: note activity and push the expiry out.
pub async fn touch(db: &PgPool, session_id: &str, lifetime: Duration) -> sqlx::Result<()> {
    sqlx::query("UPDATE sessions SET last_seen = now(), expires_at = now() + $2 WHERE id = $1").bind(session_id).bind(lifetime).execute(db).await.map(|_| ())
}

pub async fn delete(db: &PgPool, session_id: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query("DELETE FROM sessions WHERE id = $1").bind(session_id).execute(db).await?.rows_affected() > 0)
}

/// Sign a user out everywhere (password change, disable).
pub async fn delete_for_user(db: &PgPool, username: &str) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE username = $1)").bind(username).execute(db).await?;
    Ok(r.rows_affected())
}

pub async fn purge_expired(db: &PgPool) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM sessions WHERE expires_at <= now()").execute(db).await?.rows_affected())
}

/// Live sessions (all users, or one user's), newest activity first.
pub async fn list(db: &PgPool, user_id: Option<i64>) -> sqlx::Result<Vec<(String, String, String, String, DateTime<Utc>)>> {
    sqlx::query_as(
        "SELECT s.id, u.username, s.client, s.address, s.last_seen FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.expires_at > now() AND ($1::bigint IS NULL OR s.user_id = $1) ORDER BY s.last_seen DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// Owner of a session.
pub async fn owner(db: &PgPool, session_id: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT user_id FROM sessions WHERE id = $1").bind(session_id).fetch_optional(db).await
}
