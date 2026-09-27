//! Notification storage (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::{NotificationBulkAction, NotificationFilter, NotificationLevel};

use super::repo;
use super::rules::Draft;

fn draft(title: &str, level: NotificationLevel) -> Draft {
    Draft { kind: "camera_offline", camera_id: Some("cam-a".into()), level, title: title.into(), message: "m".into(), link: Some("/cameras/cam-a".into()) }
}

/// A user; returns its id.
async fn user(db: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar("INSERT INTO users (username, password_hash, role) VALUES ($1, 'x', 'admin') RETURNING id").bind(name).fetch_one(db).await.unwrap()
}

fn unread(unread_only: bool) -> NotificationFilter {
    NotificationFilter { unread_only, ..Default::default() }
}

async fn titles(db: &PgPool, me: i64, unread_only: bool) -> Vec<String> {
    repo::page(db, me, &unread(unread_only), 10, 0).await.unwrap().items.into_iter().map(|n| n.title).collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn store_page_and_mark_read(db: PgPool) {
    let me = user(&db, "ana").await;
    let a = repo::insert(&db, &draft("first", NotificationLevel::Error)).await.unwrap();
    repo::insert(&db, &draft("second", NotificationLevel::Error)).await.unwrap();
    assert_eq!(titles(&db, me, false).await, ["second", "first"], "newest first");

    repo::mark_read(&db, me, Some(&[a.id.clone()])).await.unwrap();
    let page = repo::page(&db, me, &unread(true), 10, 0).await.unwrap();
    assert_eq!((page.total, page.unread), (1, 1));
    assert_eq!(titles(&db, me, true).await, ["second"], "unread only");
    let all = repo::page(&db, me, &unread(false), 1, 1).await.unwrap();
    assert_eq!((all.total, all.unread, all.items[0].title.as_str()), (2, 1, "first"), "second page of one");

    repo::mark_read(&db, me, None).await.unwrap();
    assert_eq!(repo::page(&db, me, &unread(false), 10, 0).await.unwrap().unread, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_unread_and_delete(db: PgPool) {
    let me = user(&db, "ana").await;
    let a = repo::insert(&db, &draft("a", NotificationLevel::Info)).await.unwrap();
    let b = repo::insert(&db, &draft("b", NotificationLevel::Info)).await.unwrap();
    repo::mark_read(&db, me, None).await.unwrap();
    let ids = [a.id.clone(), b.id.clone()];
    assert_eq!(repo::bulk(&db, me, &ids, NotificationBulkAction::Unread).await.unwrap(), 2);
    assert_eq!(repo::bulk(&db, me, &ids, NotificationBulkAction::Unread).await.unwrap(), 0, "already unread");
    assert_eq!(repo::bulk(&db, me, &ids, NotificationBulkAction::Read).await.unwrap(), 2);
    assert_eq!(repo::bulk(&db, me, &[a.id], NotificationBulkAction::Delete).await.unwrap(), 1);
    assert_eq!(titles(&db, me, false).await, ["b"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn read_is_per_user(db: PgPool) {
    let (ana, dan) = (user(&db, "ana").await, user(&db, "dan").await);
    let a = repo::insert(&db, &draft("a", NotificationLevel::Warning)).await.unwrap();
    repo::insert(&db, &draft("b", NotificationLevel::Warning)).await.unwrap();
    repo::mark_read(&db, ana, None).await.unwrap();
    assert_eq!(repo::page(&db, ana, &unread(false), 10, 0).await.unwrap().unread, 0);
    assert_eq!(repo::page(&db, dan, &unread(false), 10, 0).await.unwrap().unread, 2, "dan hasn't read anything");
    repo::bulk(&db, dan, &[a.id.clone()], NotificationBulkAction::Read).await.unwrap();
    repo::bulk(&db, ana, &[a.id], NotificationBulkAction::Unread).await.unwrap();
    assert_eq!(titles(&db, dan, true).await, ["b"]);
    assert_eq!(titles(&db, ana, true).await, ["a"], "each has their own");
}

#[sqlx::test(migrations = "./migrations")]
async fn routine_ones_never_push_out_an_unread_error(db: PgPool) {
    repo::insert(&db, &draft("camera down", NotificationLevel::Error)).await.unwrap();
    for i in 0..505 {
        repo::insert(&db, &draft(&format!("info {i}"), NotificationLevel::Info)).await.unwrap();
    }
    let me = user(&db, "ana").await;
    let page = repo::page(&db, me, &unread(true), 1000, 0).await.unwrap();
    assert_eq!(page.total, 501, "500 newest plus the unread error");
    assert!(page.items.iter().any(|n| n.title == "camera down"));
    assert!(!page.items.iter().any(|n| n.title == "info 0"), "the oldest routine one went");
}

#[sqlx::test(migrations = "./migrations")]
async fn the_running_notifier_stores_a_camera_offline(db: PgPool) {
    use crate::bus::{Bus, BusEvent};
    let bus = Bus::new();
    let files = std::sync::Arc::new(crate::recordings::RecordingFiles::new(std::env::temp_dir().join("watchgrid-notifier-test")));
    super::start(db.clone(), bus.clone(), "127.0.0.1:8090".parse().unwrap(), files);
    bus.publish(BusEvent::CameraOffline { camera_id: "cam-a".into(), reason: "connection refused".into(), at: chrono::Utc::now() });
    for _ in 0..50 {
        if repo::page(&db, 0, &unread(false), 10, 0).await.unwrap().total > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(titles(&db, 0, false).await, ["cam-a is offline"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filters_by_camera_and_problems(db: PgPool) {
    let me = user(&db, "ana").await;
    repo::insert(&db, &draft("a down", NotificationLevel::Error)).await.unwrap();
    repo::insert(&db, &Draft { camera_id: Some("cam-b".into()), ..draft("b motion", NotificationLevel::Info) }).await.unwrap();
    repo::insert(&db, &Draft { camera_id: None, ..draft("disk", NotificationLevel::Warning) }).await.unwrap();
    let titles_of = |f: NotificationFilter| {
        let db = db.clone();
        async move { repo::page(&db, me, &f, 10, 0).await.unwrap() }
    };
    let b = titles_of(NotificationFilter { camera_id: Some("cam-b".into()), ..Default::default() }).await;
    assert_eq!((b.items.len(), b.items[0].title.as_str(), b.items[0].camera_id.as_deref()), (1, "b motion", Some("cam-b")));
    assert_eq!(b.unread, 3, "the bell still counts everything unread");
    let problems = titles_of(NotificationFilter { problems_only: true, ..Default::default() }).await;
    assert_eq!(problems.items.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["disk", "a down"]);
    assert_eq!(problems.total, 2);
}
