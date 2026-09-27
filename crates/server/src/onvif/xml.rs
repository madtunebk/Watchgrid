//! Namespace-agnostic helpers over ONVIF replies (matching local names,
//! since devices use every prefix imaginable).

use chrono::{DateTime, NaiveDate, Utc};
use quick_xml::Reader;
use quick_xml::events::Event;

pub(super) fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}

/// The whole text of the element just opened, up to its end tag, with
/// entities (`&amp;`, `&#38;`) and CDATA decoded, trimmed. The parser hands
/// text over in pieces around each entity: taking only the first piece
/// cut `http://cam/events?a=1&amp;b=2` to `http://cam/events?a=1`.
pub(super) fn element_text(r: &mut Reader<&[u8]>) -> Option<String> {
    let mut out = String::new();
    let mut depth = 0u32;
    loop {
        match r.read_event().ok()? {
            Event::Text(t) => out.push_str(&t.decode().ok()?),
            Event::CData(c) => out.push_str(std::str::from_utf8(&c).ok()?),
            Event::GeneralRef(g) => out.push_str(&entity(&g)?),
            Event::Start(_) => depth += 1,
            Event::End(_) if depth == 0 => return Some(out.trim().to_string()),
            Event::End(_) => depth -= 1,
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// `&amp;` → `&`, `&#38;` / `&#x26;` → `&` (XML's five names and number references).
fn entity(g: &quick_xml::events::BytesRef<'_>) -> Option<String> {
    if let Ok(Some(c)) = g.resolve_char_ref() {
        return Some(c.to_string());
    }
    Some(
        match g.decode().ok()?.as_ref() {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => return None,
        }
        .to_string(),
    )
}

/// Text of the first element with this local name.
pub fn text(xml: &str, name: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event().ok()? {
            Event::Start(e) if local(e.name().as_ref()) == name.as_bytes() => return element_text(&mut r),
            Event::Empty(e) if local(e.name().as_ref()) == name.as_bytes() => return Some(String::new()),
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// `XAddr` of the first element named `section` (e.g. "Events" in GetCapabilities).
pub fn xaddr_in(xml: &str, section: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    let mut in_section = false;
    loop {
        match r.read_event().ok()? {
            Event::Start(e) if local(e.name().as_ref()) == section.as_bytes() => in_section = true,
            Event::Start(e) if in_section && local(e.name().as_ref()) == b"XAddr" => return element_text(&mut r),
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

/// Does the reply contain this element (by local name)? `None`: not XML.
pub fn has_element(xml: &str, name: &str) -> Option<bool> {
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == name.as_bytes() => return Some(true),
            Event::Eof => return Some(false),
            _ => {}
        }
    }
}

/// Is the reply a SOAP fault: `<Fault>` as the first element in `<Body>`?
/// (Not just any element named Fault: event topics can be called that.)
pub fn is_fault(xml: &str) -> bool {
    let mut r = Reader::from_str(xml);
    let mut in_body = false;
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if !in_body && local(e.name().as_ref()) == b"Body" => in_body = true,
            Ok(Event::Start(e) | Event::Empty(e)) if in_body => return local(e.name().as_ref()) == b"Fault",
            Ok(Event::Eof) | Err(_) => return false,
            _ => {}
        }
    }
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
    /// `Source` and `Key` items (rule, zone, channel…): with the topic,
    /// they identify the property, so two rules on one topic stay apart.
    pub source: Vec<(String, String)>,
}

/// Data items that carry a detection's on/off state.
const STATE_ITEMS: [&str; 7] = ["State", "IsMotion", "IsPeople", "IsVehicle", "IsInside", "Active", "Alarm"];

fn boolean(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

impl Notification {
    /// Which property this is: the topic plus its source and key items.
    pub fn identity(&self) -> String {
        let mut parts: Vec<String> = self.source.iter().map(|(k, v)| format!("{k}={v}")).collect();
        parts.sort();
        if parts.is_empty() { self.topic.clone() } else { format!("{}[{}]", self.topic, parts.join(",")) }
    }

    /// The property no longer exists (a rule was removed, a zone emptied).
    pub fn deleted(&self) -> bool {
        self.operation.eq_ignore_ascii_case("Deleted")
    }

    /// The detection's on/off state, from an item named like one
    /// (`State`, `IsMotion`, …). Only when there is none, a single
    /// boolean-looking item is trusted; a number such as `ObjectId=1`
    /// next to others never decides.
    pub fn active(&self) -> Option<bool> {
        let named = self.data.iter().find(|(k, _)| STATE_ITEMS.iter().any(|s| k.eq_ignore_ascii_case(s)));
        if let Some((_, v)) = named {
            return boolean(v);
        }
        let mut booleans = self.data.iter().filter_map(|(_, v)| boolean(v));
        match (booleans.next(), booleans.next()) {
            (Some(b), None) => Some(b),
            _ => None,
        }
    }
}

fn strip_prefixes(topic: &str) -> String {
    topic.trim().split('/').map(|part| part.rsplit(':').next().unwrap_or(part)).collect::<Vec<_>>().join("/")
}

fn attr(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| local(a.key.as_ref()) == name).and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
}

/// All NotificationMessages in a PullMessages reply; an error when the
/// reply isn't readable XML (never a silent "no events").
pub fn notifications(xml: &str) -> Result<Vec<Notification>, String> {
    let mut r = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current: Option<Notification> = None;
    // Which part of the Message the next SimpleItems belong to.
    let (mut in_data, mut in_source) = (false, false);
    loop {
        let ev = r.read_event().map_err(|e| format!("unreadable reply: {e}"))?;
        match ev {
            // The whole topic text (entities included).
            Event::Start(e) if local(e.name().as_ref()) == b"Topic" => {
                if let (Some(n), Some(topic)) = (current.as_mut(), element_text(&mut r)) {
                    n.topic = strip_prefixes(&topic);
                }
            }
            Event::Start(e) | Event::Empty(e) => {
                let name = local(e.name().as_ref()).to_vec();
                match name.as_slice() {
                    b"NotificationMessage" => current = Some(Notification { topic: String::new(), time: None, operation: String::new(), data: Vec::new(), source: Vec::new() }),
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
                    b"Source" | b"Key" => in_source = true,
                    b"SimpleItem" if in_data || in_source => {
                        if let (Some(n), Some(k), Some(v)) = (current.as_mut(), attr(&e, b"Name"), attr(&e, b"Value")) {
                            if in_data { n.data.push((k, v)) } else { n.source.push((k, v)) }
                        }
                    }
                    _ => {}
                }
            }
            Event::End(e) => match local(e.name().as_ref()) {
                b"Data" => in_data = false,
                b"Source" | b"Key" => in_source = false,
                b"NotificationMessage" => out.extend(current.take()),
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {

    #[test]
    fn state_comes_from_its_named_item_and_bad_xml_is_an_error() {
        let n = |data: &[(&str, &str)]| super::Notification { topic: "t".into(), time: None, operation: "Changed".into(), data: data.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), source: vec![] };
        assert_eq!(n(&[("ObjectId", "1"), ("State", "false")]).active(), Some(false), "a number never decides");
        assert_eq!(n(&[("IsMotion", "true")]).active(), Some(true));
        assert_eq!(n(&[("Anything", "true")]).active(), Some(true), "a single boolean is trusted");
        assert_eq!(n(&[("ObjectId", "1"), ("Rule", "0")]).active(), None, "two unnamed booleans: undecided");
        assert!(super::notifications("<a><b></a>").is_err(), "mismatched tags: an error, not zero events");
        assert!(super::is_fault("<s:Envelope><s:Body><s:Fault><s:Reason><s:Text>no</s:Text></s:Reason></s:Fault></s:Body></s:Envelope>"));
        assert!(!super::is_fault("<s:Envelope><s:Body><tev:GetEventPropertiesResponse><tns1:Fault topic=\"true\"/></tev:GetEventPropertiesResponse></s:Body></s:Envelope>"), "a topic named Fault is not a fault");
    }

    #[test]
    fn source_items_are_kept_apart_from_data() {
        let reply = r#"<s:Envelope><s:Body><tev:PullMessagesResponse><wsnt:NotificationMessage><wsnt:Topic>tns1:RuleEngine/CellMotionDetector/Motion</wsnt:Topic><wsnt:Message><tt:Message UtcTime="2026-09-27T10:00:00Z" PropertyOperation="Deleted"><tt:Source><tt:SimpleItem Name="Rule" Value="Zone 1"/></tt:Source><tt:Key><tt:SimpleItem Name="ObjectId" Value="7"/></tt:Key></tt:Message></wsnt:Message></wsnt:NotificationMessage></tev:PullMessagesResponse></s:Body></s:Envelope>"#;
        let n = &super::notifications(reply).unwrap()[0];
        assert!(n.data.is_empty() && n.deleted());
        assert_eq!(n.identity(), "RuleEngine/CellMotionDetector/Motion[ObjectId=7,Rule=Zone 1]");
    }

    #[test]
    fn text_keeps_everything_after_an_entity() {
        let caps = r#"<s:Envelope><tds:Capabilities><tt:Events><tt:XAddr>http://cam/events?a=1&amp;b=2</tt:XAddr></tt:Events></tds:Capabilities></s:Envelope>"#;
        assert_eq!(super::xaddr_in(caps, "Events").as_deref(), Some("http://cam/events?a=1&b=2"));
        let addr = r#"<wsa5:Address>http://cam:1024/sub?id=7&amp;k=x&#38;y</wsa5:Address>"#;
        assert_eq!(super::text(addr, "Address").as_deref(), Some("http://cam:1024/sub?id=7&k=x&y"));
        assert_eq!(super::text("<Name><![CDATA[Gate & Yard]]></Name>", "Name").as_deref(), Some("Gate & Yard"));
        assert_eq!(super::text("<a><Name/></a>", "Name").as_deref(), Some(""));
    }

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
        let n = notifications(reply).unwrap();
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
