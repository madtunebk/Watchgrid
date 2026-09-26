//! SQL for notifications.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{Notification, NotificationLevel};

use super::rules::Draft;

/// How many notifications are kept.
const KEEP: i64 = 500;

fn level_name(l: NotificationLevel) -> &'static str {
    match l {
        NotificationLevel::Info => "info",
        NotificationLevel::Success => "success",
        NotificationLevel::Warning => "warning",
        NotificationLevel::Error => "error",
    }
}

fn parse_level(s: &str) -> NotificationLevel {
    match s {
        "success" => NotificationLevel::Success,
        "warning" => NotificationLevel::Warning,
        "error" => NotificationLevel::Error,
        _ => NotificationLevel::Info,
    }
}

pub async fn insert(db: &PgPool, d: &Draft) -> sqlx::Result<Notification> {
    let (id, time): (String, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO notifications (kind, camera_id, level, title, message, link) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id, created_at",
    )
    .bind(d.kind)
    .bind(&d.camera_id)
    .bind(level_name(d.level))
    .bind(&d.title)
    .bind(&d.message)
    .bind(&d.link)
    .fetch_one(db)
    .await?;
    sqlx::query("DELETE FROM notifications WHERE id IN (SELECT id FROM notifications ORDER BY created_at DESC OFFSET $1)").bind(KEEP).execute(db).await?;
    Ok(Notification { id, level: d.level, title: d.title.clone(), message: d.message.clone(), time, read: false, link: d.link.clone() })
}

/// Newest first.
pub async fn list(db: &PgPool, limit: i64) -> sqlx::Result<Vec<Notification>> {
    let rows: Vec<(String, String, String, String, DateTime<Utc>, bool, Option<String>)> =
        sqlx::query_as("SELECT id, level, title, message, created_at, read, link FROM notifications ORDER BY created_at DESC, id DESC LIMIT $1")
            .bind(limit)
            .fetch_all(db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, level, title, message, time, read, link)| Notification { id, level: parse_level(&level), title, message, time, read, link })
        .collect())
}

/// Mark some (or, with `None`, all) as read.
pub async fn mark_read(db: &PgPool, ids: Option<&[String]>) -> sqlx::Result<()> {
    match ids {
        Some(ids) => sqlx::query("UPDATE notifications SET read = TRUE WHERE id = ANY($1)").bind(ids).execute(db).await,
        None => sqlx::query("UPDATE notifications SET read = TRUE WHERE NOT read").execute(db).await,
    }
    .map(|_| ())
}
