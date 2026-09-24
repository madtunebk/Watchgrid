//! Which Watchgrid event types an ONVIF topic can produce.

use watchgrid_model::EventType;

pub fn detection(topic: &str) -> Option<EventType> {
    let t = topic.to_ascii_lowercase();
    if t.contains("people") || t.contains("person") || t.contains("human") || t.contains("face") {
        Some(EventType::Person)
    } else if t.contains("vehicle") || t.contains("car") || t.contains("licenseplate") {
        Some(EventType::Vehicle)
    } else if t.contains("animal") || t.contains("pet") {
        Some(EventType::Animal)
    } else if t.contains("motion") || t.contains("intrusion") || t.contains("linecross") || t.contains("fielddetector") {
        Some(EventType::Motion)
    } else {
        None
    }
}

/// Distinct detections in a stable order.
pub fn detections(topics: &[String]) -> Vec<EventType> {
    let mut out: Vec<EventType> = Vec::new();
    for kind in topics.iter().filter_map(|t| detection(t)) {
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out.sort_by_key(|k| EventType::ALL.iter().position(|x| x == k));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_vendor_topics() {
        let topics: Vec<String> = ["RuleEngine/CellMotionDetector/Motion", "RuleEngine/MyRuleDetector/PeopleDetect", "RuleEngine/MyRuleDetector/VehicleDetect", "Device/Trigger/DigitalInput"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(detections(&topics), [EventType::Motion, EventType::Person, EventType::Vehicle]);
        assert_eq!(detection("Device/Trigger/DigitalInput"), None);
    }
}
