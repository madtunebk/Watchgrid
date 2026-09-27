//! SQL for notifications. Read / unread is per user (`notification_reads`);
//! the notifications themselves (and deleting them) are shared.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use watchgrid_model::{Notification, NotificationBulkAction, NotificationFilter, NotificationLevel, NotificationPage};

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
    Ok(Notification { id, level: d.level, title: d.title.clone(), message: d.message.clone(), time, read: false, link: d.link.clone(), camera_id: d.camera_id.clone() })
}

/// id, level, title, message, created_at, read, link, camera_id
type Row = (String, String, String, String, DateTime<Utc>, bool, Option<String>, Option<String>);

/// SQL: `$u` has read notification `n`.
const READ_BY: &str = "EXISTS (SELECT 1 FROM notification_reads r WHERE r.notification_id = n.id AND r.user_id = $1)";

async fn prune(db: &PgPool) -> sqlx::Result<()> {
    // Warnings and errors nobody has read yet stay beyond KEEP.
    sqlx::query(
        "DELETE FROM notifications n WHERE id IN (
             SELECT id FROM notifications ORDER BY created_at DESC, id DESC OFFSET $1
         ) AND (level NOT IN ('warning', 'error') OR EXISTS (SELECT 1 FROM notification_reads r WHERE r.notification_id = n.id))",
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

/// A page for `user`, newest first, matching `f`. `unread` counts all of
/// the user's unread ones (the bell), whatever the filter.
pub async fn page(db: &PgPool, user: i64, f: &NotificationFilter, limit: i64, offset: i64) -> sqlx::Result<NotificationPage> {
    let matches = format!(
        "NOT ($2 AND {READ_BY}) AND ($3::text IS NULL OR camera_id = $3) AND (NOT $4 OR level IN ('warning', 'error'))"
    );
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT id, level, title, message, created_at, {READ_BY} AS read, link, camera_id FROM notifications n
         WHERE {matches} ORDER BY created_at DESC, id DESC LIMIT $5 OFFSET $6"
    ))
    .bind(user)
    .bind(f.unread_only)
    .bind(&f.camera_id)
    .bind(f.problems_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let (total, unread): (i64, i64) = sqlx::query_as(&format!(
        "SELECT COUNT(*) FILTER (WHERE {matches}), COUNT(*) FILTER (WHERE NOT {READ_BY}) FROM notifications n"
    ))
    .bind(user)
    .bind(f.unread_only)
    .bind(&f.camera_id)
    .bind(f.problems_only)
    .fetch_one(db)
    .await?;
    Ok(NotificationPage {
        items: rows
            .into_iter()
            .map(|(id, level, title, message, time, read, link, camera_id)| Notification { id, level: parse_level(&level), title, message, time, read, link, camera_id })
            .collect(),
        total: total as u32,
        unread: unread as u32,
    })
}

/// Mark read / unread for `user`, or delete (for everyone); returns how
/// many changed.
pub async fn bulk(db: &PgPool, user: i64, ids: &[String], action: NotificationBulkAction) -> sqlx::Result<u32> {
    let done = match action {
        NotificationBulkAction::Read => {
            sqlx::query("INSERT INTO notification_reads (notification_id, user_id) SELECT id, $2 FROM notifications WHERE id = ANY($1) ON CONFLICT DO NOTHING")
                .bind(ids)
                .bind(user)
                .execute(db)
                .await?
        }
        NotificationBulkAction::Unread => {
            sqlx::query("DELETE FROM notification_reads WHERE notification_id = ANY($1) AND user_id = $2").bind(ids).bind(user).execute(db).await?
        }
        NotificationBulkAction::Delete => sqlx::query("DELETE FROM notifications WHERE id = ANY($1)").bind(ids).execute(db).await?,
    };
    Ok(done.rows_affected() as u32)
}

/// Mark some (or, with `None`, all) as read for `user`.
pub async fn mark_read(db: &PgPool, user: i64, ids: Option<&[String]>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO notification_reads (notification_id, user_id)
         SELECT id, $1 FROM notifications WHERE $2::text[] IS NULL OR id = ANY($2) ON CONFLICT DO NOTHING",
    )
    .bind(user)
    .bind(ids)
    .execute(db)
    .await
    .map(|_| ())
}
