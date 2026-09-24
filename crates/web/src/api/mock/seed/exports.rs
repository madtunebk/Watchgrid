use crate::api::{AutoUpload, ExportKind, ExportTarget};

pub fn targets() -> Vec<ExportTarget> {
    let t = |id: &str, name: &str, kind, location: &str, problem: Option<&str>, auto_upload| ExportTarget {
        id: id.into(),
        name: name.into(),
        kind,
        location: location.into(),
        ready: problem.is_none(),
        problem: problem.map(Into::into),
        auto_upload,
    };
    vec![
        t("gdrive", "Google Drive", ExportKind::GoogleDrive, "My Drive / Watchgrid", None, AutoUpload::Person),
        t("minio", "NAS backup (MinIO)", ExportKind::S3, "s3://nvr-backup/clips", None, AutoUpload::Protected),
        t("nextcloud", "Nextcloud", ExportKind::Nextcloud, "cloud.example.home / Cameras", Some("Sign-in expired — reconnect in Settings"), AutoUpload::Off),
    ]
}
