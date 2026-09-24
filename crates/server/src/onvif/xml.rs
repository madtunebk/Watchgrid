//! Namespace-agnostic helpers over ONVIF replies (matching local names,
//! since devices use every prefix imaginable).

use chrono::{DateTime, NaiveDate, Utc};
use quick_xml::Reader;
use quick_xml::events::Event;

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}

/// Text of the first element with this local name.
pub fn text(xml: &str, name: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    let mut inside = false;
    loop {
        match r.read_event().ok()? {
            Event::Start(e) if local(e.name().as_ref()) == name.as_bytes() => inside = true,
            Event::Text(t) if inside => return t.decode().ok().map(|s| s.trim().to_string()),
            Event::End(_) if inside => return Some(String::new()),
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// `XAddr` of the first element named `section` (e.g. "Events" in GetCapabilities).
pub fn xaddr_in(xml: &str, section: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    let (mut in_section, mut in_xaddr) = (false, false);
    loop {
        match r.read_event().ok()? {
            Event::Start(e) if local(e.name().as_ref()) == section.as_bytes() => in_section = true,
            Event::Start(e) if in_section && local(e.name().as_ref()) == b"XAddr" => in_xaddr = true,
            Event::Text(t) if in_xaddr => return t.decode().ok().map(|s| s.trim().to_string()),
            Event::End(e) if local(e.name().as_ref()) == section.as_bytes() => in_section = false,
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// Leaf topics of a TopicSet: paths of elements marked `topic="true"`.
pub fn topics(xml: &str) -> Vec<String> {
    let mut r = Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut in_set = false;
    let mut out = Vec::new();
    loop {
        let Ok(ev) = r.read_event() else { break };
        match ev {
            Event::Start(e) if !in_set => {
                if local(e.name().as_ref()) == b"TopicSet" {
                    in_set = true;
                }
            }
            Event::Empty(_) if !in_set => {}
            Event::Start(e) => {
                let name = String::from_utf8_lossy(local(e.name().as_ref())).into_owned();
                let leaf = e.attributes().flatten().any(|a| local(a.key.as_ref()) == b"topic" && a.value.as_ref() == b"true");
                stack.push(name);
                if leaf {
                    out.push(stack.join("/"));
                }
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(local(e.name().as_ref())).into_owned();
                if e.attributes().flatten().any(|a| local(a.key.as_ref()) == b"topic" && a.value.as_ref() == b"true") {
                    out.push(stack.iter().cloned().chain([name]).collect::<Vec<_>>().join("/"));
                }
            }
            Event::End(e) => {
                if local(e.name().as_ref()) == b"TopicSet" && stack.is_empty() {
                    in_set = false;
                } else {
                    stack.pop();
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// SOAP fault text, if any.
pub fn fault_reason(xml: &str) -> Option<String> {
    text(xml, "Text").or_else(|| text(xml, "Value")).filter(|s| !s.is_empty())
}

/// The UTC time in a GetSystemDateAndTime reply.
pub fn device_utc_time(xml: &str) -> Option<DateTime<Utc>> {
    let start = xml.find("UTCDateTime")?;
    let part = &xml[start..];
    let n = |name: &str| text(part, name)?.parse::<u32>().ok();
    let date = NaiveDate::from_ymd_opt(n("Year")? as i32, n("Month")?, n("Day")?)?;
    Some(date.and_hms_opt(n("Hour")?, n("Minute")?, n("Second")?)?.and_utc())
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROPS: &str = r#"<env:Envelope xmlns:env="e" xmlns:wstop="w" xmlns:tns1="t"><env:Body><tev:GetEventPropertiesResponse xmlns:tev="v">
        <wstop:TopicSet>
          <tns1:RuleEngine><CellMotionDetector><Motion wstop:topic="true"><tt:MessageDescription/></Motion></CellMotionDetector>
            <MyRuleDetector><PeopleDetect wstop:topic="true"/></MyRuleDetector></tns1:RuleEngine>
          <tns1:VideoSource><MotionAlarm wstop:topic="true"/></tns1:VideoSource>
        </wstop:TopicSet></tev:GetEventPropertiesResponse></env:Body></env:Envelope>"#;

    #[test]
    fn extracts_leaf_topics() {
        assert_eq!(topics(PROPS), ["RuleEngine/CellMotionDetector/Motion", "RuleEngine/MyRuleDetector/PeopleDetect", "VideoSource/MotionAlarm"]);
    }

    #[test]
    fn finds_texts_addresses_and_time() {
        let caps = "<a:Capabilities><a:Device><a:XAddr>http://d/dev</a:XAddr></a:Device><a:Events><a:XAddr>http://d/events</a:XAddr></a:Events></a:Capabilities>";
        assert_eq!(xaddr_in(caps, "Events").as_deref(), Some("http://d/events"));
        assert_eq!(text("<x:Manufacturer> Hikvision </x:Manufacturer>", "Manufacturer").as_deref(), Some("Hikvision"));
        let t = "<tt:UTCDateTime><tt:Time><tt:Hour>21</tt:Hour><tt:Minute>5</tt:Minute><tt:Second>9</tt:Second></tt:Time><tt:Date><tt:Year>2026</tt:Year><tt:Month>9</tt:Month><tt:Day>24</tt:Day></tt:Date></tt:UTCDateTime>";
        assert_eq!(device_utc_time(t), Some("2026-09-24T21:05:09Z".parse().unwrap()));
    }
}
