use crate::api::{AutoUpload, ExportKind};
use crate::ui::I;

pub fn kind(k: ExportKind) -> &'static str {
    match k {
        ExportKind::S3 => "S3 / MinIO",
        ExportKind::Nextcloud => "Nextcloud (WebDAV)",
    }
}

pub fn icon(k: ExportKind) -> I {
    match k {
        ExportKind::S3 => I::Database,
        ExportKind::Nextcloud => I::Cloud,
    }
}

pub const AUTO: [(AutoUpload, &str, &str); 5] = [
    (AutoUpload::Off, "off", "Manual only"),
    (AutoUpload::Protected, "protected", "Protected events"),
    (AutoUpload::Motion, "motion", "Motion detections"),
    (AutoUpload::Person, "person", "Person detections"),
    (AutoUpload::AllEvents, "all", "Every event"),
];
