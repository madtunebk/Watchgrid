//! SQL for notifications.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{Notification, NotificationBulkAction, NotificationLevel, NotificationPage};

use super::rules::Draft;

/// How many notifications are kept. Unread warnings and errors are kept
/// beyond that (a flood of routine ones must not push them out), up to
/// `HARD_CAP` in all.
const KEEP: i64 = 500;
const HARD_CAP: i64 = 2000;

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
    prune(db).await?;
    Ok(Notification { id, level: d.level, title: d.title.clone(), message: d.message.clone(), time, read: false, link: d.link.clone() })
}

/// id, level, title, message, created_at, read, link
type Row = (String, String, String, String, DateTime<Utc>, bool, Option<String>);

async fn prune(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM notifications WHERE id IN (
             SELECT id FROM notifications ORDER BY created_at DESC, id DESC OFFSET $1
         ) AND (read OR level NOT IN ('warning', 'error'))",
    )
    .bind(KEEP)
    .execute(db)
    .await?;
    sqlx::query("DELETE FROM notifications WHERE id IN (SELECT id FROM notifications ORDER BY created_at DESC, id DESC OFFSET $1)")
        .bind(HARD_CAP)
        .execute(db)
        .await
        .map(|_| ())
}

/// A page, newest first; `unread_only` filters.
pub async fn page(db: &PgPool, unread_only: bool, limit: i64, offset: i64) -> sqlx::Result<NotificationPage> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, level, title, message, created_at, read, link FROM notifications
         WHERE NOT ($1 AND read) ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
    )
    .bind(unread_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let (total, unread): (i64, i64) =
        sqlx::query_as("SELECT COUNT(*) FILTER (WHERE NOT ($1 AND read)), COUNT(*) FILTER (WHERE NOT read) FROM notifications").bind(unread_only).fetch_one(db).await?;
    Ok(NotificationPage {
        items: rows
            .into_iter()
            .map(|(id, level, title, message, time, read, link)| Notification { id, level: parse_level(&level), title, message, time, read, link })
            .collect(),
        total: total as u32,
        unread: unread as u32,
    })
}

/// Mark read / unread, or delete; returns how many changed.
pub async fn bulk(db: &PgPool, ids: &[String], action: NotificationBulkAction) -> sqlx::Result<u32> {
    let sql = match action {
        NotificationBulkAction::Read => "UPDATE notifications SET read = TRUE WHERE id = ANY($1) AND NOT read",
        NotificationBulkAction::Unread => "UPDATE notifications SET read = FALSE WHERE id = ANY($1) AND read",
        NotificationBulkAction::Delete => "DELETE FROM notifications WHERE id = ANY($1)",
    };
    Ok(sqlx::query(sql).bind(ids).execute(db).await?.rows_affected() as u32)
}

/// Mark some (or, with `None`, all) as read.
pub async fn mark_read(db: &PgPool, ids: Option<&[String]>) -> sqlx::Result<()> {
    match ids {
        Some(ids) => sqlx::query("UPDATE notifications SET read = TRUE WHERE id = ANY($1)").bind(ids).execute(db).await,
        None => sqlx::query("UPDATE notifications SET read = TRUE WHERE NOT read").execute(db).await,
    }
    .map(|_| ())
}
