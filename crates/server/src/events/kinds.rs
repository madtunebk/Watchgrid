//! Event types as stored in the database (same names as the JSON API).

use watchgrid_model::EventType;

pub fn name(kind: EventType) -> &'static str {
    match kind {
        EventType::Motion => "motion",
        EventType::Person => "person",
        EventType::Vehicle => "vehicle",
        EventType::Animal => "animal",
        EventType::Onvif => "onvif",
        EventType::Manual => "manual",
        EventType::Scheduled => "scheduled",
        EventType::Api => "api",
        EventType::CameraOffline => "camera_offline",
        EventType::CameraOnline => "camera_online",
        EventType::Security => "security",
    }
}

pub fn parse(s: &str) -> Option<EventType> {
    EventType::ALL.into_iter().find(|k| name(*k) == s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_match_the_json_api() {
        for kind in EventType::ALL {
            assert_eq!(parse(name(kind)), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), name(kind));
        }
        assert_eq!(parse("nope"), None);
    }
}
