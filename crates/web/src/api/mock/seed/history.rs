//! Past events and their recordings, generated deterministically.

use chrono::{DateTime, Duration, Utc};

use super::super::sim::Rng;
use crate::api::{Camera, CameraStatus, Detection, Event, EventType, LastEventSummary, Recording, RecordingMode, RecordingReason};
use crate::clock::start_of_today;

const DAYS: i64 = 7;

/// Event types each camera tends to produce.
fn typical(camera_id: &str) -> &'static [EventType] {
    match camera_id {
        "cam-front" => &[EventType::Person, EventType::Person, EventType::Motion],
        "cam-driveway" => &[EventType::Vehicle, EventType::Motion, EventType::Person],
        "cam-backyard" => &[EventType::Animal, EventType::Motion, EventType::Manual],
        "cam-garage" => &[EventType::Motion, EventType::Onvif],
        _ => &[EventType::Manual, EventType::Motion],
    }
}

pub fn generate(cameras: &[Camera]) -> (Vec<Event>, Vec<Recording>) {
    let mut rng = Rng::new(42);
    let now = Utc::now();
    let today = start_of_today();
    let mut events = Vec::new();
    let mut recordings = Vec::new();

    for cam in cameras {
        for day in 0..DAYS {
            // An offline camera has nothing recent.
            if cam.status != CameraStatus::Online && day < 1 {
                continue;
            }
            let day_start = today - Duration::days(day);
            let day_end = if day == 0 { now - Duration::minutes(5) } else { day_start + Duration::days(1) };
            let span = (day_end - day_start).num_minutes().max(1) as u32;
            let count = if day == 0 { rng.range(3, 5) } else { rng.range(4, 9) };

            for _ in 0..count {
                let start = day_start + Duration::minutes(rng.range(0, span) as i64) + Duration::seconds(rng.range(0, 60) as i64);
                let kind = rng.pick(typical(&cam.id));
                let duration = if kind == EventType::Manual { rng.range(60, 400) } else { rng.range(8, 120) };
                push(&mut events, &mut recordings, cam, kind, start, Some(duration), &mut rng);
            }
        }
        if cam.recording.mode == RecordingMode::Continuous {
            continuous(&mut recordings, cam, today, now);
        }
        // The "live" part of the seed: events currently in progress.
        if cam.motion_active {
            push(&mut events, &mut recordings, cam, EventType::Person, now - Duration::minutes(2), None, &mut rng);
        }
        if cam.recording_reason == Some(RecordingReason::Manual) {
            push(&mut events, &mut recordings, cam, EventType::Manual, now - Duration::minutes(7), None, &mut rng);
        }
    }

    events.sort_by(|a, b| b.start_time.cmp(&a.start_time));
    recordings.sort_by(|a, b| b.start_time.cmp(&a.start_time));
    (events, recordings)
}

fn push(
    events: &mut Vec<Event>,
    recordings: &mut Vec<Recording>,
    cam: &Camera,
    kind: EventType,
    start: DateTime<Utc>,
    duration: Option<u32>,
    rng: &mut Rng,
) {
    let n = events.len();
    let (id, rec_id) = (format!("evt-{n:05}"), format!("rec-{n:05}"));
    let end = duration.map(|d| start + Duration::seconds(d as i64));
    let secs = duration.unwrap_or_else(|| (Utc::now() - start).num_seconds().max(0) as u32);
    let pre_post = cam.recording.pre_record_seconds + cam.recording.post_record_seconds;
    let bytes_per_sec = cam.main_stream.bitrate.unwrap_or(4096) as u64 * 1000 / 8;

    let detections = match kind {
        EventType::Person => vec![Detection { label: "person".into(), confidence: 0.78 + rng.range(0, 20) as f32 / 100.0 }],
        EventType::Vehicle => vec![Detection { label: "car".into(), confidence: 0.81 + rng.range(0, 17) as f32 / 100.0 }],
        EventType::Animal => vec![Detection { label: "cat".into(), confidence: 0.66 + rng.range(0, 25) as f32 / 100.0 }],
        _ => vec![],
    };
    let source = match kind {
        EventType::Manual => "Manual (web UI)",
        EventType::Api => "HTTP API",
        _ => "ONVIF RuleEngine/CellMotionDetector",
    };

    events.push(Event {
        id: id.clone(),
        camera_id: cam.id.clone(),
        kind,
        start_time: start,
        end_time: end,
        duration: secs,
        recording_id: Some(rec_id.clone()),
        thumbnail: None,
        protected: n % 23 == 0,
        detections,
        source: source.into(),
    });
    recordings.push(Recording {
        id: rec_id,
        camera_id: cam.id.clone(),
        start_time: start - Duration::seconds(cam.recording.pre_record_seconds as i64),
        end_time: end.map(|e| e + Duration::seconds(cam.recording.post_record_seconds as i64)),
        duration: secs + pre_post,
        reason: match kind {
            EventType::Manual => RecordingReason::Manual,
            EventType::Motion => RecordingReason::Motion,
            _ => RecordingReason::Event,
        },
        file_size: (secs + pre_post) as u64 * bytes_per_sec,
        protected: false,
        protected_by_events: u32::from(n % 23 == 0),
        event_ids: vec![id],
    });
}

/// Fill `Camera::last_event` from the generated history.
pub fn attach_last_events(cameras: &mut [Camera], events: &[Event]) {
    for cam in cameras {
        cam.last_event = events.iter().filter(|e| e.camera_id == cam.id).max_by_key(|e| e.start_time).map(|e| {
            LastEventSummary { event_id: e.id.clone(), kind: e.kind, time: e.start_time }
        });
    }
}

/// Hour-long continuous segments over the whole history, with a short
/// outage every few days so gaps show on the timeline.
fn continuous(recordings: &mut Vec<Recording>, cam: &Camera, today: DateTime<Utc>, now: DateTime<Utc>) {
    let bytes_per_sec = cam.main_stream.bitrate.unwrap_or(4096) as u64 * 1000 / 8;
    let mut t = today - Duration::days(DAYS - 1);
    let mut n = 0;
    while t < now {
        n += 1;
        let end = (t + Duration::hours(1)).min(now);
        // Simulated outages only in the past, never on the live segment.
        let outage = n % 53 == 0 && end < now - Duration::hours(1);
        if !outage {
            let secs = (end - t).num_seconds().max(0) as u32;
            let live = end == now;
            recordings.push(Recording {
                id: format!("rec-c-{}-{n}", cam.id),
                camera_id: cam.id.clone(),
                start_time: t,
                end_time: (!live).then_some(end),
                duration: secs,
                reason: RecordingReason::Continuous,
                file_size: secs as u64 * bytes_per_sec,
                protected: false,
                protected_by_events: 0,
                event_ids: vec![],
            });
        }
        t = end;
    }
}
