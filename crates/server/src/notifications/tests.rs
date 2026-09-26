//! Notification storage (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::{NotificationBulkAction, NotificationLevel};

use super::repo;
use super::rules::Draft;

fn draft(title: &str, level: NotificationLevel) -> Draft {
    Draft { kind: "camera_offline", camera_id: Some("cam-a".into()), level, title: title.into(), message: "m".into(), link: Some("/cameras/cam-a".into()) }
}

async fn titles(db: &PgPool, unread_only: bool) -> Vec<String> {
    repo::page(db, unread_only, 10, 0).await.unwrap().items.into_iter().map(|n| n.title).collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn store_page_and_mark_read(db: PgPool) {
    let a = repo::insert(&db, &draft("first", NotificationLevel::Error)).await.unwrap();
    repo::insert(&db, &draft("second", NotificationLevel::Error)).await.unwrap();
    assert_eq!(titles(&db, false).await, ["second", "first"], "newest first");

    repo::mark_read(&db, Some(&[a.id.clone()])).await.unwrap();
    let page = repo::page(&db, true, 10, 0).await.unwrap();
    assert_eq!((page.total, page.unread), (1, 1));
    assert_eq!(titles(&db, true).await, ["second"], "unread only");
    let all = repo::page(&db, false, 1, 1).await.unwrap();
    assert_eq!((all.total, all.unread, all.items[0].title.as_str()), (2, 1, "first"), "second page of one");

    repo::mark_read(&db, None).await.unwrap();
    assert_eq!(repo::page(&db, false, 10, 0).await.unwrap().unread, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_unread_and_delete(db: PgPool) {
    let a = repo::insert(&db, &draft("a", NotificationLevel::Info)).await.unwrap();
    let b = repo::insert(&db, &draft("b", NotificationLevel::Info)).await.unwrap();
    repo::mark_read(&db, None).await.unwrap();
    let ids = [a.id.clone(), b.id.clone()];
    assert_eq!(repo::bulk(&db, &ids, NotificationBulkAction::Unread).await.unwrap(), 2);
    assert_eq!(repo::bulk(&db, &ids, NotificationBulkAction::Unread).await.unwrap(), 0, "already unread");
    assert_eq!(repo::bulk(&db, &[a.id], NotificationBulkAction::Delete).await.unwrap(), 1);
    assert_eq!(titles(&db, false).await, ["b"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn routine_ones_never_push_out_an_unread_error(db: PgPool) {
    repo::insert(&db, &draft("camera down", NotificationLevel::Error)).await.unwrap();
    for i in 0..505 {
        repo::insert(&db, &draft(&format!("info {i}"), NotificationLevel::Info)).await.unwrap();
    }
    let page = repo::page(&db, true, 1000, 0).await.unwrap();
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
        if repo::page(&db, false, 10, 0).await.unwrap().total > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(titles(&db, false).await, ["cam-a is offline"]);
}
