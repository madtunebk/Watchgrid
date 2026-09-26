//! Export destinations and jobs in the database (`sqlx::test`).

use sqlx::PgPool;
use watchgrid_model::{AutoUpload, ExportKind, ExportState};

use super::repo::{self, StoredTarget};

fn target(id: &str) -> StoredTarget {
    StoredTarget {
        id: id.into(),
        name: "MinIO".into(),
        kind: ExportKind::S3,
        endpoint: "http://minio.lan:9000".into(),
        location: "bucket/clips".into(),
        username: "AKID".into(),
        secret_enc: Some(vec![1, 2, 3]),
        auto_upload: AutoUpload::Person,
        problem: None,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn targets_never_expose_secrets_and_jobs_dedupe(db: PgPool) {
    let id = repo::new_target_id(&db).await.unwrap();
    repo::insert_target(&db, &target(&id)).await.unwrap();
    let public = repo::targets(&db).await.unwrap()[0].public();
    let json = serde_json::to_string(&public).unwrap();
    assert!(!json.contains("AKID") && !json.contains("minio.lan"), "no credentials or endpoint in the API: {json}");
    assert!(public.ready);

    let job = repo::insert_job(&db, Some("evt-1"), "rec-1", &id).await.unwrap().expect("queued");
    assert_eq!(job.state, ExportState::Queued);
    assert!(repo::insert_job(&db, Some("evt-2"), "rec-1", &id).await.unwrap().is_none(), "never a second live job for the same clip + destination");
    assert_eq!(repo::existing_job(&db, "rec-1", &id).await.unwrap().map(|j| j.id), Some(job.id.clone()), "same clip + destination reuses the job");

    repo::job_started(&db, &job.id, 1000).await.unwrap();
    repo::job_progress(&db, &job.id, 250).await.unwrap();
    assert_eq!(repo::job_by_id(&db, &job.id).await.unwrap().unwrap().progress, 25.0);
    repo::job_finished(&db, &job.id, &Err("HTTP 403".into())).await.unwrap();
    assert!(repo::existing_job(&db, "rec-1", &id).await.unwrap().is_none(), "failed jobs can be retried");

    repo::set_problem(&db, &id, Some("bad key")).await.unwrap();
    assert!(!repo::targets(&db).await.unwrap()[0].public().ready);
    assert!(repo::delete_target(&db, &id).await.unwrap());
    assert!(repo::job_by_id(&db, &job.id).await.unwrap().is_none(), "jobs go with their destination");
}

#[sqlx::test(migrations = "./migrations")]
async fn a_failed_job_can_be_queued_again(db: PgPool) {
    let id = repo::new_target_id(&db).await.unwrap();
    repo::insert_target(&db, &target(&id)).await.unwrap();
    let first = repo::insert_job(&db, None, "rec-1", &id).await.unwrap().unwrap();
    repo::job_finished(&db, &first.id, &Err("timeout".into())).await.unwrap();
    assert!(repo::insert_job(&db, None, "rec-1", &id).await.unwrap().is_some(), "retry after a failure");
}
