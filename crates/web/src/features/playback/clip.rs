//! What the player plays: a time span on one camera, optionally with a
//! highlighted part (the event inside its pre/post-record buffers).

use chrono::{DateTime, Utc};

use crate::api::{self, Event, EventType, Recording};

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub camera_name: String,
    pub start: DateTime<Utc>,
    /// Seconds.
    pub duration: f32,
    /// `(offset, length)` in seconds of the part worth watching.
    pub highlight: Option<(f32, f32)>,
    /// Tag shown on the video (event type).
    pub kind: Option<EventType>,
    /// Draw the detection box while inside the highlight.
    pub detection_box: bool,
    /// Real video to play; `None` shows the simulated preview player.
    pub src: Option<String>,
    /// The recording is still being written: the file grows.
    pub growing: bool,
}

impl Clip {
    /// An event inside its recording (pre-record + event + post-record).
    pub fn for_event(event: &Event, recording: Option<&Recording>, camera_name: String) -> Self {
        let (start, duration) = match recording {
            Some(r) => (r.start_time, r.duration.max(1) as f32),
            None => (event.start_time, event.duration.max(1) as f32),
        };
        let offset = (event.start_time - start).num_milliseconds().max(0) as f32 / 1000.0;
        let len = (event.duration as f32).min(duration - offset).max(0.0);
        Self {
            camera_name,
            start,
            duration,
            highlight: Some((offset, len)),
            kind: Some(event.kind),
            detection_box: !event.detections.is_empty(),
            src: recording.and_then(|r| api::recording_media_url(&r.id)),
            growing: recording.is_some_and(|r| r.end_time.is_none()),
        }
    }

    /// A whole recording, highlighting its first event if it has one.
    pub fn for_recording(recording: &Recording, first_event: Option<&Event>, camera_name: String) -> Self {
        let src = api::recording_media_url(&recording.id);
        match first_event {
            Some(e) => Self { src, ..Self::for_event(e, Some(recording), camera_name) },
            None => Self {
                camera_name,
                start: recording.start_time,
                duration: recording.duration.max(1) as f32,
                highlight: None,
                kind: None,
                detection_box: false,
                src,
                growing: recording.end_time.is_none(),
            },
        }
    }
}
