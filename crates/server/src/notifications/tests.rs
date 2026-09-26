//! Notification storage (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::NotificationLevel;

use super::repo;
use super::rules::Draft;

fn draft(title: &str) -> Draft {
    Draft { kind: "camera_offline", camera_id: Some("cam-a".into()), level: NotificationLevel::Error, title: title.into(), message: "m".into(), link: Some("/cameras/cam-a".into()) }
}

#[sqlx::test(migrations = "./migrations")]
async fn store_list_and_mark_read(db: PgPool) {
    let a = repo::insert(&db, &draft("first")).await.unwrap();
    let b = repo::insert(&db, &draft("second")).await.unwrap();
    let list = repo::list(&db, 10).await.unwrap();
    assert_eq!(list.iter().map(|n| n.title.as_str()).collect::<Vec<_>>(), ["second", "first"], "newest first");
    assert!(list.iter().all(|n| !n.read));

    repo::mark_read(&db, Some(&[a.id.clone()])).await.unwrap();
    let list = repo::list(&db, 10).await.unwrap();
    assert_eq!(list.iter().find(|n| n.id == a.id).map(|n| n.read), Some(true));
    assert_eq!(list.iter().find(|n| n.id == b.id).map(|n| n.read), Some(false));
    repo::mark_read(&db, None).await.unwrap();
    assert!(repo::list(&db, 10).await.unwrap().iter().all(|n| n.read));
}
