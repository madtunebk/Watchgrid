//! Exporting clips to external destinations (cloud drives, object storage).
//! Credentials live on the server only; the UI sees names and status.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    GoogleDrive,
    /// Any S3-compatible store: AWS S3, MinIO, Wasabi, Backblaze B2…
    S3,
    /// WebDAV (Nextcloud / ownCloud).
    Nextcloud,
    Dropbox,
}

/// What gets uploaded without anyone clicking Export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpload {
    Off,
    /// Events marked protected.
    Protected,
    /// Person detections.
    Person,
    /// Every event clip.
    AllEvents,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTarget {
    pub id: Id,
    pub name: String,
    pub kind: ExportKind,
    /// Where files go, e.g. "My Drive / Watchgrid" or "s3://nvr-backup/clips".
    pub location: String,
    /// Credentials valid and destination reachable.
    pub ready: bool,
    /// Why it is not ready (expired sign-in, unreachable…).
    pub problem: Option<String>,
    pub auto_upload: AutoUpload,
}

/// Body of `POST /exports/targets`. Secrets are write-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTargetInput {
    pub name: String,
    pub kind: ExportKind,
    /// S3 / Nextcloud server URL. OAuth kinds leave it empty.
    pub endpoint: String,
    /// Bucket (S3) or folder path.
    pub location: String,
    pub username: String,
    pub secret: Option<String>,
    pub auto_upload: AutoUpload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    Queued,
    Uploading,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJob {
    pub id: Id,
    pub event_id: Id,
    pub target_id: Id,
    pub state: ExportState,
    /// 0-100
    pub progress: f32,
    /// Link to the uploaded file when the destination provides one.
    pub link: Option<String>,
    pub message: Option<String>,
    pub created_at: Timestamp,
}
