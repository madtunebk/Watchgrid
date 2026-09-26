use crate::api::{AutoUpload, ExportKind};
use crate::ui::I;

pub fn kind(k: ExportKind) -> &'static str {
    match k {
        ExportKind::GoogleDrive => "Google Drive",
        ExportKind::S3 => "S3 / MinIO",
        ExportKind::Nextcloud => "Nextcloud (WebDAV)",
        ExportKind::Dropbox => "Dropbox",
    }
}

pub fn icon(k: ExportKind) -> I {
    match k {
        ExportKind::S3 => I::Database,
        _ => I::Cloud,
    }
}

/// OAuth services sign in through their own page instead of keys.
pub fn uses_oauth(k: ExportKind) -> bool {
    matches!(k, ExportKind::GoogleDrive | ExportKind::Dropbox)
}

pub const AUTO: [(AutoUpload, &str, &str); 5] = [
    (AutoUpload::Off, "off", "Manual only"),
    (AutoUpload::Protected, "protected", "Protected events"),
    (AutoUpload::Motion, "motion", "Motion detections"),
    (AutoUpload::Person, "person", "Person detections"),
    (AutoUpload::AllEvents, "all", "Every event"),
];
