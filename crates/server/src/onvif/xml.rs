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

/// One event from a PullMessages reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    /// Without namespace prefixes, e.g. "RuleEngine/CellMotionDetector/Motion".
    pub topic: String,
    pub time: Option<DateTime<Utc>>,
    /// "Initialized", "Changed" or "Deleted".
    pub operation: String,
    /// `Data` items as (name, value).
    pub data: Vec<(String, String)>,
}

impl Notification {
    /// The first boolean data value (IsMotion, IsPeople, State, …).
    pub fn active(&self) -> Option<bool> {
        self.data.iter().find_map(|(_, v)| match v.to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        })
    }
}

fn strip_prefixes(topic: &str) -> String {
    topic.trim().split('/').map(|part| part.rsplit(':').next().unwrap_or(part)).collect::<Vec<_>>().join("/")
}

fn attr(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| local(a.key.as_ref()) == name).and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
}

/// All NotificationMessages in a PullMessages reply.
pub fn notifications(xml: &str) -> Vec<Notification> {
    let mut r = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current: Option<Notification> = None;
    let (mut in_topic, mut in_data) = (false, false);
    loop {
        let Ok(ev) = r.read_event() else { break };
        match ev {
            Event::Start(e) | Event::Empty(e) => {
                let name = local(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"NotificationMessage" => current = Some(Notification { topic: String::new(), time: None, operation: String::new(), data: Vec::new() }),
                    b"Topic" => in_topic = true,
                    b"Message" if current.is_some() => {
                        if let Some(n) = current.as_mut() {
                            if let Some(t) = attr(&e, b"UtcTime") {
                                n.time = t.parse().ok();
                            }
                            if let Some(op) = attr(&e, b"PropertyOperation") {
                                n.operation = op;
                            }
                        }
                    }
                    b"Data" => in_data = true,
                    b"SimpleItem" if in_data => {
                        if let (Some(n), Some(k), Some(v)) = (current.as_mut(), attr(&e, b"Name"), attr(&e, b"Value")) {
                            n.data.push((k, v));
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(t) if in_topic => {
                if let (Some(n), Ok(text)) = (current.as_mut(), t.decode()) {
                    n.topic = strip_prefixes(&text);
                }
            }
            Event::End(e) => match local(e.name().as_ref()) {
                b"Topic" => in_topic = false,
                b"Data" => in_data = false,
                b"NotificationMessage" => out.extend(current.take()),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    out
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
    fn parses_pull_messages() {
        let reply = r#"<env:Body><tev:PullMessagesResponse><tev:CurrentTime>x</tev:CurrentTime>
          <wsnt:NotificationMessage><wsnt:Topic Dialect="d">tns1:RuleEngine/CellMotionDetector/Motion</wsnt:Topic>
            <wsnt:Message><tt:Message UtcTime="2026-09-24T21:00:05Z" PropertyOperation="Changed">
              <tt:Source><tt:SimpleItem Name="VideoSourceConfigurationToken" Value="vsconf"/></tt:Source>
              <tt:Data><tt:SimpleItem Name="IsMotion" Value="true"/></tt:Data></tt:Message></wsnt:Message></wsnt:NotificationMessage>
          <wsnt:NotificationMessage><wsnt:Topic>tns1:RuleEngine/PeopleDetector/People</wsnt:Topic>
            <wsnt:Message><tt:Message UtcTime="2026-09-24T21:00:07Z" PropertyOperation="Initialized">
              <tt:Data><tt:SimpleItem Name="IsPeople" Value="false"/></tt:Data></tt:Message></wsnt:Message></wsnt:NotificationMessage>
          </tev:PullMessagesResponse></env:Body>"#;
        let n = notifications(reply);
        assert_eq!(n.len(), 2);
        assert_eq!((n[0].topic.as_str(), n[0].active(), n[0].operation.as_str()), ("RuleEngine/CellMotionDetector/Motion", Some(true), "Changed"));
        assert_eq!(n[0].time, Some("2026-09-24T21:00:05Z".parse().unwrap()));
        assert_eq!(n[0].data, [("IsMotion".to_string(), "true".to_string())], "source items are not data");
        assert_eq!((n[1].topic.as_str(), n[1].active()), ("RuleEngine/PeopleDetector/People", Some(false)));
    }

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
