//! Which Watchgrid event types an ONVIF topic can produce.
//!
//! Topics are matched word by word (`RuleEngine/LineCrossDetector/LineCross`
//! → rule, engine, line, cross, detector…), never by substring: "car" must
//! not make `Device/Hardware/CartridgeStatus` a vehicle.

use watchgrid_model::EventType;

const PERSON: &[&str] = &["people", "person", "persons", "human", "humans", "face", "faces", "pedestrian"];
const VEHICLE: &[&str] = &["vehicle", "vehicles", "car", "cars", "truck", "licenseplate", "plate"];
const ANIMAL: &[&str] = &["animal", "animals", "pet", "pets", "dog", "cat"];
/// Motion-like detections: moving pixels, a crossed line, an entered area.
const MOTION: &[&str] = &["motion", "intrusion", "linecross", "crossed", "crossing", "field", "loitering"];

pub fn detection(topic: &str) -> Option<EventType> {
    let words = words(topic);
    let has = |list: &[&str]| words.iter().any(|w| list.iter().any(|k| matches(w, k)));
    if has(PERSON) {
        Some(EventType::Person)
    } else if has(VEHICLE) {
        Some(EventType::Vehicle)
    } else if has(ANIMAL) {
        Some(EventType::Animal)
    } else if has(MOTION) || (words.iter().any(|w| w == "line") && words.iter().any(|w| w == "cross")) {
        Some(EventType::Motion)
    } else {
        None
    }
}

/// A word is the keyword, or starts with it when the keyword is long enough
/// not to hide inside other words (`motiondetected` is motion; `cartridge`
/// is not a car, nor `petrol` a pet).
fn matches(word: &str, keyword: &str) -> bool {
    word == keyword || (keyword.len() >= 5 && word.starts_with(keyword))
}

/// Lowercase words of a topic: split at `/`, `:`, `_`, `-` and CamelCase
/// (`TPSmartEventDetector` → tp, smart, event, detector), plus each
/// segment whole (`LicensePlate` → licenseplate too).
fn words(topic: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in topic.split(['/', ':', '_', '-', '.']).filter(|s| !s.is_empty()) {
        out.push(segment.to_ascii_lowercase());
        let chars: Vec<char> = segment.chars().collect();
        let mut word = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let prev = i.checked_sub(1).map(|p| chars[p]);
            let next = chars.get(i + 1);
            // A new word starts at lower→Upper, and at the last capital of
            // an acronym followed by lowercase ("TPSmart" → TP, Smart).
            let boundary = c.is_ascii_uppercase()
                && prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit() || (p.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase())));
            if boundary && !word.is_empty() {
                out.push(std::mem::take(&mut word).to_ascii_lowercase());
            }
            word.push(c);
        }
        if !word.is_empty() {
            out.push(word.to_ascii_lowercase());
        }
    }
    out
}

/// Topics that report someone signing in to the camera with a wrong
/// password (e.g. `UserAlarm/IllegalAccess`).
pub fn security(topic: &str) -> bool {
    words(topic).iter().any(|w| w == "illegalaccess")
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

    #[test]
    fn the_topics_our_cameras_advertise() {
        // Tapo TC71/TC72 and the ezviz.
        assert_eq!(detection("RuleEngine/CellMotionDetector/Motion"), Some(EventType::Motion));
        assert_eq!(detection("RuleEngine/PeopleDetector/People"), Some(EventType::Person));
        assert_eq!(detection("RuleEngine/LineCrossDetector/LineCross"), Some(EventType::Motion));
        assert_eq!(detection("RuleEngine/IntrusionDetector/Intrusion"), Some(EventType::Motion));
        assert_eq!(detection("RuleEngine/TamperDetector/Tamper"), None);
        assert_eq!(detection("RuleEngine/TPSmartEventDetector/TPSmartEvent"), None, "no kind in the name");
        assert_eq!(detection("VideoSource/MotionAlarm"), Some(EventType::Motion));
    }

    #[test]
    fn words_not_substrings() {
        assert_eq!(detection("Device/Hardware/CartridgeStatus"), None, "car inside cartridge");
        assert_eq!(detection("Monitoring/ProcessorUsage"), None);
        assert_eq!(detection("Device/Trigger/Relay/Carrier"), None);
        assert_eq!(detection("RuleEngine/Petrol/Level"), None, "pet inside petrol");
        assert_eq!(detection("RuleEngine/ObjectDetector/Car"), Some(EventType::Vehicle));
        assert_eq!(detection("RuleEngine/LicensePlateRecognition/Plate"), Some(EventType::Vehicle));
        assert_eq!(detection("RuleEngine/FieldDetector/ObjectsInside"), Some(EventType::Motion));
        assert_eq!(detection("tns1:RuleEngine/FaceDetector/Face"), Some(EventType::Person));
        assert_eq!(detection("VideoAnalytics/motiondetected"), Some(EventType::Motion), "lowercase compound");
        assert_eq!(detection("Alarm/humandetection"), Some(EventType::Person));
        assert_eq!(detection("Status/facetime"), None, "short keywords only whole");
    }

    #[test]
    fn splits_camel_case_and_acronyms() {
        assert_eq!(words("TPSmartEvent"), ["tpsmartevent", "tp", "smart", "event"]);
        assert_eq!(words("RuleEngine/IsMotion"), ["ruleengine", "rule", "engine", "ismotion", "is", "motion"]);
    }

    #[test]
    fn illegal_access_is_a_security_topic_not_a_detection() {
        assert!(security("UserAlarm/IllegalAccess"));
        assert!(!security("RuleEngine/CellMotionDetector/Motion"));
        assert_eq!(detection("UserAlarm/IllegalAccess"), None);
    }
}
