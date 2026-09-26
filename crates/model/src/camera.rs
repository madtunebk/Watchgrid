//! Cameras: connection, streams, recording and motion settings, live state.

use serde::{Deserialize, Serialize};

use crate::{EventType, Id, RecordingReason, Timestamp};

/// Connectivity of the camera itself. Independent of streaming, motion and
/// recording, which are tracked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraStatus {
    Online,
    Offline,
    Connecting,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamStatus {
    Active,
    Idle,
    Error,
    Unconfigured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    Disabled,
    /// Only records when started by hand (UI or API).
    Manual,
    /// Default: record on motion / events with pre- and post-buffers.
    Events,
    Continuous,
    Scheduled,
}

impl RecordingMode {
    pub const ALL: [Self; 5] = [Self::Disabled, Self::Manual, Self::Events, Self::Continuous, Self::Scheduled];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionSource {
    /// Motion reported by the camera (ONVIF events).
    Onvif,
    /// Motion computed by the NVR.
    Software,
    /// Object detection. Future.
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamRole {
    Main,
    Sub,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub url: String,
    pub status: StreamStatus,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    /// kbit/s
    pub bitrate: Option<u32>,
    pub audio_codec: Option<String>,
}

impl Stream {
    pub fn unprobed(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            status: StreamStatus::Idle,
            codec: None,
            width: None,
            height: None,
            fps: None,
            bitrate: None,
            audio_codec: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnvifConfig {
    pub url: String,
    pub username: String,
    /// Write-only: accepted on input, never returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastEventSummary {
    pub event_id: Id,
    pub kind: EventType,
    pub time: Timestamp,
}

/// Longest pre-record the recorder keeps (its buffer holds this much).
pub const MAX_PRE_RECORD_SECONDS: u32 = 30;
/// Longest post-record.
pub const MAX_POST_RECORD_SECONDS: u32 = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSettings {
    pub mode: RecordingMode,
    /// Which stream is written to disk.
    pub stream: StreamRole,
    pub pre_record_seconds: u32,
    pub post_record_seconds: u32,
    /// Events shorter than this are discarded.
    pub min_event_seconds: u32,
    /// Long events are split into clips of at most this length.
    pub max_clip_seconds: u32,
    /// Events closer together than this are merged into one recording.
    pub event_merge_seconds: u32,
    /// When "scheduled" mode records (server time zone).
    #[serde(default)]
    pub schedule: Vec<ScheduleWindow>,
    /// Keep this camera's recordings at most this many days (on top of the
    /// global retention policy). `None`: only the global policy applies.
    #[serde(default)]
    pub retention_days: Option<u32>,
    /// Save the camera's sound with the video (when it has any; main stream
    /// only).
    #[serde(default = "record_audio_default")]
    pub record_audio: bool,
}

fn record_audio_default() -> bool {
    true
}

/// A weekly recording window. `end` before `start` runs past midnight into
/// the next day; `start == end` means the whole day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleWindow {
    /// Days the window starts on: 0 = Monday … 6 = Sunday.
    pub days: Vec<u8>,
    /// Minutes after midnight, 0–1439.
    pub start_minute: u16,
    pub end_minute: u16,
}

impl ScheduleWindow {
    /// Is `minute` (0–1439) of `weekday` (0 = Monday) inside the window?
    pub fn contains(&self, weekday: u8, minute: u16) -> bool {
        let on = |d: u8| self.days.contains(&d);
        let yesterday = (weekday + 6) % 7;
        if self.start_minute == self.end_minute {
            on(weekday)
        } else if self.start_minute < self.end_minute {
            on(weekday) && (self.start_minute..self.end_minute).contains(&minute)
        } else {
            (on(weekday) && minute >= self.start_minute) || (on(yesterday) && minute < self.end_minute)
        }
    }
}

/// Is any window active at this moment?
pub fn schedule_active(windows: &[ScheduleWindow], weekday: u8, minute: u16) -> bool {
    windows.iter().any(|w| w.contains(weekday, minute))
}

impl Default for RecordingSettings {
    fn default() -> Self {
        Self {
            mode: RecordingMode::Events,
            stream: StreamRole::Main,
            pre_record_seconds: 5,
            post_record_seconds: 15,
            min_event_seconds: 2,
            max_clip_seconds: 600,
            event_merge_seconds: 10,
            schedule: Vec::new(),
            retention_days: None,
            record_audio: true,
        }
    }
}

/// Rectangular motion zone in normalised (0..1) frame coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionZone {
    pub id: Id,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Exclusion zones mask out motion instead of triggering on it.
    pub exclude: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionSettings {
    pub enabled: bool,
    pub source: MotionSource,
    /// 0-100
    pub sensitivity: u8,
    /// Which stream motion detection analyses. Falls back to main when there is no substream.
    pub stream: StreamRole,
    pub zones: Vec<MotionZone>,
    /// Also raise a notification for plain motion (people and vehicles
    /// follow Settings → Notifications). Off unless chosen for this camera.
    #[serde(default)]
    pub notify: bool,
}

impl Default for MotionSettings {
    fn default() -> Self {
        Self { enabled: true, source: MotionSource::Onvif, sensitivity: 60, stream: StreamRole::Main, zones: Vec::new(), notify: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub location: String,
    pub enabled: bool,
    pub status: CameraStatus,
    pub host: String,
    pub username: String,
    pub main_stream: Stream,
    /// Optional. A camera with a single RTSP stream is fully supported.
    pub sub_stream: Option<Stream>,
    pub onvif: Option<OnvifConfig>,

    pub recording: RecordingSettings,
    pub motion: MotionSettings,

    /// Live state: a recording is being written right now.
    pub recording_active: bool,
    /// Live state: why the current recording runs.
    pub recording_reason: Option<RecordingReason>,
    /// Live state: motion is being detected right now.
    pub motion_active: bool,

    pub last_event: Option<LastEventSummary>,
    /// Bytes used by this camera's recordings.
    pub storage_used: Option<u64>,
    pub connected_since: Option<Timestamp>,
    pub created_at: Timestamp,
}

impl Camera {
    /// Is the live stream actually delivering video?
    pub fn streaming(&self) -> bool {
        self.enabled && self.status == CameraStatus::Online && self.main_stream.status == StreamStatus::Active
    }
}

/// Body of `POST /cameras` and `PUT /cameras/{id}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraInput {
    pub name: String,
    pub description: String,
    pub location: String,
    pub enabled: bool,
    pub host: String,
    pub username: String,
    /// `None` on update keeps the stored password.
    pub password: Option<String>,
    pub main_stream_url: String,
    pub sub_stream_url: Option<String>,
    pub onvif: Option<OnvifConfig>,
    pub recording: RecordingSettings,
    pub motion: MotionSettings,
}

impl From<&Camera> for CameraInput {
    /// Editable view of an existing camera. Secrets are never echoed back,
    /// so `password` is `None` ("keep the stored one").
    fn from(c: &Camera) -> Self {
        Self {
            name: c.name.clone(),
            description: c.description.clone(),
            location: c.location.clone(),
            enabled: c.enabled,
            host: c.host.clone(),
            username: c.username.clone(),
            password: None,
            main_stream_url: c.main_stream.url.clone(),
            sub_stream_url: c.sub_stream.as_ref().map(|s| s.url.clone()),
            onvif: c.onvif.as_ref().map(|o| OnvifConfig { password: None, ..o.clone() }),
            recording: c.recording.clone(),
            motion: c.motion.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(days: &[u8], start: u16, end: u16) -> ScheduleWindow {
        ScheduleWindow { days: days.to_vec(), start_minute: start, end_minute: end }
    }

    #[test]
    fn windows_within_a_day_and_past_midnight() {
        let office = w(&[0, 1, 2, 3, 4], 8 * 60, 18 * 60);
        assert!(office.contains(0, 9 * 60));
        assert!(!office.contains(0, 18 * 60), "end is exclusive");
        assert!(!office.contains(5, 9 * 60), "not on Saturday");

        let night = w(&[4], 22 * 60, 6 * 60); // Friday 22:00 → Saturday 06:00
        assert!(night.contains(4, 23 * 60));
        assert!(night.contains(5, 5 * 60), "continues into Saturday morning");
        assert!(!night.contains(5, 23 * 60), "but doesn't start on Saturday");
        assert!(night.contains(4, 22 * 60) && !night.contains(4, 21 * 60));

        let sunday_all_day = w(&[6], 0, 0);
        assert!(sunday_all_day.contains(6, 0) && sunday_all_day.contains(6, 1439) && !sunday_all_day.contains(0, 0));
        assert!(schedule_active(&[office, night], 5, 60));
        assert!(!schedule_active(&[], 0, 0));
    }
}
