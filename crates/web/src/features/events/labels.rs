use crate::api::EventType;
use crate::ui::I;

/// Uppercase tag, e.g. "PERSON".
pub fn tag(kind: EventType) -> &'static str {
    match kind {
        EventType::Motion => "MOTION",
        EventType::Person => "PERSON",
        EventType::Vehicle => "VEHICLE",
        EventType::Animal => "ANIMAL",
        EventType::Onvif => "ONVIF",
        EventType::Manual => "MANUAL",
        EventType::Api => "API",
    }
}

/// Sentence form, e.g. "Person detected".
pub fn title(kind: EventType) -> &'static str {
    match kind {
        EventType::Motion => "Motion detected",
        EventType::Person => "Person detected",
        EventType::Vehicle => "Vehicle detected",
        EventType::Animal => "Animal detected",
        EventType::Onvif => "ONVIF event",
        EventType::Manual => "Manual recording",
        EventType::Api => "External trigger",
    }
}

/// CSS modifier for the event colour.
pub fn css(kind: EventType) -> &'static str {
    match kind {
        EventType::Motion => "motion",
        EventType::Person => "person",
        EventType::Vehicle => "vehicle",
        EventType::Animal => "animal",
        EventType::Onvif => "onvif",
        EventType::Manual => "manual",
        EventType::Api => "api",
    }
}

/// Parse the key produced by [`css`] (used in URLs too).
pub fn from_key(key: &str) -> Option<EventType> {
    EventType::ALL.into_iter().find(|k| css(*k) == key)
}

pub fn icon(kind: EventType) -> I {
    match kind {
        EventType::Motion => I::Activity,
        EventType::Person => I::User,
        EventType::Vehicle => I::Car,
        EventType::Animal => I::PawPrint,
        EventType::Onvif => I::Radio,
        EventType::Manual => I::RecordDot,
        EventType::Api => I::Plug,
    }
}

/// Types that need detection features not built yet; shown but marked.
pub fn is_future(kind: EventType) -> bool {
    matches!(kind, EventType::Animal | EventType::Api)
}
