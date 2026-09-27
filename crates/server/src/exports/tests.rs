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

#[sqlx::test(migrations = "./migrations")]
async fn an_edit_keeps_the_id_and_clears_the_problem(db: PgPool) {
    let id = repo::new_target_id(&db).await.unwrap();
    repo::insert_target(&db, &target(&id)).await.unwrap();
    repo::set_problem(&db, &id, Some("bad key")).await.unwrap();
    let job = repo::insert_job(&db, None, "rec-1", &id).await.unwrap().unwrap();

    let edited = StoredTarget { location: "other/prefix".into(), secret_enc: Some(vec![4, 5]), ..target(&id) };
    assert!(repo::update_target(&db, &edited).await.unwrap());
    let saved = repo::target_by_id(&db, &id).await.unwrap().unwrap();
    assert_eq!(saved.location, "other/prefix");
    assert_eq!(saved.secret_enc, Some(vec![4, 5]));
    assert!(saved.problem.is_none(), "a tested edit is ready");
    assert!(saved.settings().has_secret);
    assert!(!serde_json::to_string(&saved.settings()).unwrap().contains("[4,5]"), "the secret never leaves");
    assert!(repo::job_by_id(&db, &job.id).await.unwrap().is_some(), "jobs stay with the edited destination");
    assert!(!repo::update_target(&db, &target("exp-missing")).await.unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn a_failed_upload_waits_for_its_retry(db: PgPool) {
    let id = repo::new_target_id(&db).await.unwrap();
    repo::insert_target(&db, &target(&id)).await.unwrap();
    let job = repo::insert_job(&db, None, "rec-1", &id).await.unwrap().unwrap();
    repo::job_started(&db, &job.id, 1000).await.unwrap();

    let later = chrono::Utc::now() + chrono::Duration::minutes(5);
    repo::job_retry_later(&db, &job.id, "HTTP 503", later).await.unwrap();
    let waiting = repo::job_by_id(&db, &job.id).await.unwrap().unwrap();
    assert_eq!(waiting.state, ExportState::Queued);
    assert_eq!(waiting.message.as_deref(), Some("HTTP 503"), "the last error stays visible");
    assert_eq!(repo::job_attempts(&db, &job.id).await.unwrap(), 1);
    assert!(repo::insert_job(&db, None, "rec-1", &id).await.unwrap().is_none(), "still the one live job");
    assert!(repo::take_due_retries(&db).await.unwrap().is_empty(), "not due yet");
    assert!(repo::requeue_unfinished(&db).await.unwrap().is_empty(), "a restart keeps the wait");

    assert!(repo::retry_now(&db, &job.id).await.unwrap(), "asked again: due now");
    assert!(repo::job_by_id(&db, &job.id).await.unwrap().unwrap().message.is_none(), "no longer waiting");
    assert!(!repo::retry_now(&db, &job.id).await.unwrap());
    repo::job_retry_later(&db, &job.id, "HTTP 503", chrono::Utc::now() - chrono::Duration::seconds(1)).await.unwrap();
    assert_eq!(repo::take_due_retries(&db).await.unwrap(), vec![job.id.clone()]);
    assert!(repo::take_due_retries(&db).await.unwrap().is_empty(), "handed out once");
    assert_eq!(repo::job_attempts(&db, &job.id).await.unwrap(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_cancelled_job_never_starts_and_leaves_the_pending_list(db: PgPool) {
    let id = repo::new_target_id(&db).await.unwrap();
    repo::insert_target(&db, &target(&id)).await.unwrap();
    let waiting = repo::insert_job(&db, None, "rec-1", &id).await.unwrap().unwrap();
    let other = repo::insert_job(&db, None, "rec-2", &id).await.unwrap().unwrap();
    assert_eq!(repo::active_jobs(&db).await.unwrap().len(), 2);

    assert!(repo::cancel_queued(&db, &waiting.id).await.unwrap());
    assert!(!repo::job_started(&db, &waiting.id, 100).await.unwrap(), "a worker skips it");
    let cancelled = repo::job_by_id(&db, &waiting.id).await.unwrap().unwrap();
    assert_eq!((cancelled.state, cancelled.message.as_deref()), (ExportState::Failed, Some("Cancelled")));
    assert!(repo::insert_job(&db, None, "rec-1", &id).await.unwrap().is_some(), "it can be exported again");

    assert!(repo::job_started(&db, &other.id, 100).await.unwrap());
    assert!(!repo::cancel_queued(&db, &other.id).await.unwrap(), "a running one is stopped by the service, not here");
    let active: Vec<String> = repo::active_jobs(&db).await.unwrap().into_iter().map(|j| j.id).collect();
    assert_eq!(active.len(), 2, "the running one and the new one");
    assert!(!active.contains(&waiting.id));
}
