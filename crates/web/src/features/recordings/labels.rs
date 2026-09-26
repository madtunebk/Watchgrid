use crate::api::RecordingReason;

/// Timeline colour groups: motion, manual, continuous, everything else.
pub fn css(reason: RecordingReason) -> &'static str {
    match reason {
        RecordingReason::Motion => "motion",
        RecordingReason::Manual => "manual",
        RecordingReason::Continuous => "continuous",
        RecordingReason::Event | RecordingReason::Scheduled | RecordingReason::Api => "other",
    }
}

pub fn label(reason: RecordingReason) -> &'static str {
    match reason {
        RecordingReason::Motion => "Motion",
        RecordingReason::Event => "Event",
        RecordingReason::Manual => "Manual",
        RecordingReason::Continuous => "Continuous",
        RecordingReason::Scheduled => "Scheduled",
        RecordingReason::Api => "API",
    }
}

pub const LEGEND: [(&str, &str); 4] = [("motion", "Motion"), ("manual", "Manual"), ("continuous", "Continuous"), ("other", "Other events")];
