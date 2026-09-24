//! SQL for users. Only the CLI creates, changes or deletes them.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{Role, User};

pub struct StoredUser {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub enabled: bool,
    pub last_login: Option<DateTime<Utc>>,
}

pub fn role_name(r: Role) -> &'static str {
    match r {
        Role::Admin => "admin",
        Role::Viewer => "viewer",
    }
}

pub fn parse_role(s: &str) -> Option<Role> {
    match s {
        "admin" => Some(Role::Admin),
        "viewer" => Some(Role::Viewer),
        _ => None,
    }
}

/// Lowercase letters, digits, `.`, `_`, `-`; 1–32 characters.
pub fn valid_username(name: &str) -> bool {
    (1..=32).contains(&name.len()) && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
}

type Row = (i64, String, String, String, bool, Option<DateTime<Utc>>);

fn from_row((id, username, password_hash, role, enabled, last_login): Row) -> StoredUser {
    StoredUser { id, username, password_hash, role: parse_role(&role).unwrap_or(Role::Viewer), enabled, last_login }
}

const COLUMNS: &str = "id, username, password_hash, role, enabled, last_login";

pub async fn find(db: &PgPool, username: &str) -> sqlx::Result<Option<StoredUser>> {
    let row: Option<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM users WHERE username = $1")).bind(username).fetch_optional(db).await?;
    Ok(row.map(from_row))
}

pub async fn all(db: &PgPool) -> sqlx::Result<Vec<StoredUser>> {
    let rows: Vec<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM users ORDER BY username")).fetch_all(db).await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub async fn count(db: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(db).await
}

pub async fn create(db: &PgPool, username: &str, password_hash: &str, role: Role) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3)")
        .bind(username)
        .bind(password_hash)
        .bind(role_name(role))
        .execute(db)
        .await
        .map(|_| ())
}

/// Returns whether the user exists.
pub async fn set_password(db: &PgPool, username: &str, password_hash: &str) -> sqlx::Result<bool> {
    let r = sqlx::query("UPDATE users SET password_hash = $2 WHERE username = $1").bind(username).bind(password_hash).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn set_enabled(db: &PgPool, username: &str, enabled: bool) -> sqlx::Result<bool> {
    let r = sqlx::query("UPDATE users SET enabled = $2 WHERE username = $1").bind(username).bind(enabled).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

/// Deletes the user and (by cascade) their sessions.
pub async fn delete(db: &PgPool, username: &str) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM users WHERE username = $1").bind(username).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn touch_login(db: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET last_login = now() WHERE id = $1").bind(id).execute(db).await.map(|_| ())
}

impl StoredUser {
    /// What the web UI may see (never the hash).
    pub fn public(&self) -> User {
        User { id: self.id.to_string(), username: self.username.clone(), role: self.role, last_login: self.last_login }
    }
}
