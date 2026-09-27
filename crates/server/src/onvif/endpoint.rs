//! A subscription's endpoint reference (WS-Addressing): the address that
//! Pull/Renew/Unsubscribe go to, the reference parameters the device wants
//! echoed back as SOAP headers on each of them, and the lease it granted.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::xml::{element_text, local, text};

const WSA: &str = "http://www.w3.org/2005/08/addressing";

#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub address: String,
    /// Header elements, each self-contained (the namespaces it uses are
    /// declared on it) and marked `IsReferenceParameter`.
    pub parameters: Vec<String>,
}

/// The `SubscriptionReference` of a CreatePullPointSubscription reply: its
/// own `Address` (not the first Address anywhere in the reply) and every
/// child of its `ReferenceParameters`.
pub fn subscription_reference(xml: &str) -> Option<Endpoint> {
    let mut r = Reader::from_str(xml);
    // Namespace declarations of each open element, to carry into the copies.
    let mut scopes: Vec<Vec<(String, String)>> = Vec::new();
    let mut reference: Option<usize> = None; // depth of SubscriptionReference
    let mut in_parameters = false;
    let mut address = None;
    let mut parameters = Vec::new();
    loop {
        let start = r.buffer_position() as usize;
        match r.read_event().ok()? {
            Event::Start(e) => {
                let depth = scopes.len();
                let name = local(e.name().as_ref()).to_vec();
                if in_parameters {
                    // One reference parameter: copy it whole.
                    let inherited = in_scope(&scopes);
                    r.read_to_end(e.name()).ok()?;
                    parameters.push(self_contained(&xml[start..r.buffer_position() as usize], &inherited)?);
                    continue;
                }
                scopes.push(declarations(&e));
                match reference {
                    None if name == b"SubscriptionReference" => reference = Some(depth),
                    Some(d) if depth == d + 1 && name == b"Address" => {
                        address = element_text(&mut r);
                        scopes.pop();
                    }
                    Some(d) if depth == d + 1 && name == b"ReferenceParameters" => in_parameters = true,
                    _ => {}
                }
            }
            Event::Empty(_) if in_parameters => {
                parameters.push(self_contained(&xml[start..r.buffer_position() as usize], &in_scope(&scopes))?);
            }
            Event::End(_) => {
                scopes.pop();
                if in_parameters {
                    in_parameters = false; // </ReferenceParameters>
                } else if reference == Some(scopes.len()) {
                    break; // </SubscriptionReference>
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Some(Endpoint { address: address.filter(|a| !a.is_empty())?, parameters })
}

/// `xmlns` / `xmlns:p` attributes of one start tag, as (prefix, uri).
fn declarations(e: &BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .filter_map(|a| {
            let key = std::str::from_utf8(a.key.as_ref()).ok()?;
            let prefix = if key == "xmlns" { "" } else { key.strip_prefix("xmlns:")? };
            Some((prefix.to_string(), a.unescape_value().ok()?.into_owned()))
        })
        .collect()
}

/// Every namespace declared by the open elements; inner ones win.
fn in_scope(scopes: &[Vec<(String, String)>]) -> BTreeMap<String, String> {
    scopes.iter().flatten().cloned().collect()
}

/// The element's source with the inherited namespaces declared on it (the
/// ones it doesn't declare itself) and the WS-Addressing marker, so it
/// means the same once moved into our SOAP header.
fn self_contained(element: &str, inherited: &BTreeMap<String, String>) -> Option<String> {
    let name_end = element[1..].find(|c: char| c.is_whitespace() || c == '>' || c == '/')? + 1;
    let own: Vec<String> = {
        let mut r = Reader::from_str(element);
        match r.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) => declarations(&e).into_iter().map(|(p, _)| p).collect(),
            _ => return None,
        }
    };
    let mut added = String::new();
    for (prefix, uri) in inherited.iter().filter(|(p, _)| !own.contains(p)) {
        let attr = if prefix.is_empty() { "xmlns".to_string() } else { format!("xmlns:{prefix}") };
        added.push_str(&format!(r#" {attr}="{}""#, super::xml::escape(uri)));
    }
    added.push_str(&format!(r#" xmlns:wgwsa="{WSA}" wgwsa:IsReferenceParameter="true""#));
    Some(format!("{}{added}{}", &element[..name_end], &element[name_end..]))
}

/// How long the subscription lives, from a Create or Renew reply:
/// TerminationTime minus CurrentTime, both on the camera's clock, so a
/// wrong camera clock doesn't matter. `None` when the reply doesn't say
/// (or says something impossible); then the requested lifetime applies.
pub fn granted_lease(xml: &str) -> Option<Duration> {
    let time = |name| text(xml, name)?.parse::<DateTime<Utc>>().ok();
    let lease = (time("TerminationTime")? - time("CurrentTime")?).to_std().ok()?;
    (lease > Duration::ZERO && lease <= Duration::from_secs(24 * 3600)).then_some(lease)
}

/// When to renew a lease: halfway, but at least 10 s before it ends (a
/// pull may hold the connection for 5 s first), and no sooner than 1 s.
pub fn renew_after(lease: Duration) -> Duration {
    (lease / 2).min(lease.saturating_sub(Duration::from_secs(10))).max(Duration::from_secs(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa5="http://www.w3.org/2005/08/addressing" xmlns:dev="urn:vendor:sub"><env:Body><tev:CreatePullPointSubscriptionResponse xmlns:tev="http://www.onvif.org/ver10/events/wsdl"><tev:SubscriptionReference><wsa5:Address>http://10.0.0.1:1026/event?id=7&amp;x=1</wsa5:Address><wsa5:ReferenceParameters><dev:SubId>42</dev:SubId><dev:Token kind="a"/></wsa5:ReferenceParameters><wsa5:Metadata><wsa5:Address>http://wrong/</wsa5:Address></wsa5:Metadata></tev:SubscriptionReference><wsnt:CurrentTime xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2">1970-01-01T00:10:00Z</wsnt:CurrentTime><wsnt:TerminationTime xmlns:wsnt="http://docs.oasis-open.org/wsn/b-2">1970-01-01T00:11:00Z</wsnt:TerminationTime></tev:CreatePullPointSubscriptionResponse></env:Body></env:Envelope>"#;

    #[test]
    fn the_subscription_s_own_address_and_its_parameters() {
        let e = subscription_reference(REPLY).unwrap();
        assert_eq!(e.address, "http://10.0.0.1:1026/event?id=7&x=1", "decoded, and not Metadata's Address");
        assert_eq!(e.parameters.len(), 2);
        let p = &e.parameters[0];
        assert!(p.starts_with("<dev:SubId ") && p.ends_with(">42</dev:SubId>"), "{p}");
        assert!(p.contains(r#"xmlns:dev="urn:vendor:sub""#), "the prefix it uses travels with it: {p}");
        assert!(p.contains(r#"wgwsa:IsReferenceParameter="true""#), "{p}");
        assert!(e.parameters[1].starts_with("<dev:Token ") && e.parameters[1].contains(r#"kind="a""#) && e.parameters[1].ends_with("/>"));
    }

    #[test]
    fn a_copied_parameter_is_valid_xml_on_its_own() {
        for p in subscription_reference(REPLY).unwrap().parameters {
            let mut r = quick_xml::NsReader::from_str(&p);
            loop {
                match r.read_resolved_event().unwrap() {
                    (quick_xml::name::ResolveResult::Bound(ns), Event::Start(_) | Event::Empty(_)) => assert_eq!(ns.as_ref(), b"urn:vendor:sub"),
                    (_, Event::Eof) => break,
                    (_, Event::End(_) | Event::Text(_)) => {}
                    other => panic!("unbound element in {p}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn a_declaration_the_parameter_makes_itself_is_not_repeated() {
        let xml = r#"<Env xmlns:a="urn:outer"><SubscriptionReference><Address>http://cam/s</Address><ReferenceParameters><a:Id xmlns:a="urn:inner">1</a:Id></ReferenceParameters></SubscriptionReference></Env>"#;
        let p = &subscription_reference(xml).unwrap().parameters[0];
        assert!(p.contains("urn:inner") && !p.contains("urn:outer"), "{p}");
    }

    #[test]
    fn most_cameras_have_no_parameters() {
        let xml = r#"<Body><SubscriptionReference><Address>http://cam/s</Address></SubscriptionReference></Body>"#;
        assert_eq!(subscription_reference(xml), Some(Endpoint { address: "http://cam/s".into(), parameters: vec![] }));
        assert_eq!(subscription_reference("<Body><Address>http://cam/s</Address></Body>"), None, "an Address outside a SubscriptionReference is not one");
        assert_eq!(subscription_reference("<SubscriptionReference><Address/></SubscriptionReference>"), None);
    }

    #[test]
    fn the_lease_comes_from_the_camera_s_own_clock() {
        assert_eq!(granted_lease(REPLY), Some(Duration::from_secs(60)), "a camera stuck in 1970 still grants 60 s");
        assert_eq!(granted_lease("<R><TerminationTime>2026-01-01T00:00:00Z</TerminationTime></R>"), None, "no CurrentTime");
        assert_eq!(granted_lease("<R><CurrentTime>2026-01-01T00:01:00Z</CurrentTime><TerminationTime>2026-01-01T00:00:00Z</TerminationTime></R>"), None, "already over");
    }

    #[test]
    fn renewals_leave_room_for_a_pull() {
        assert_eq!(renew_after(Duration::from_secs(300)), Duration::from_secs(150));
        assert_eq!(renew_after(Duration::from_secs(60)), Duration::from_secs(30));
        assert_eq!(renew_after(Duration::from_secs(16)), Duration::from_secs(6));
        assert_eq!(renew_after(Duration::from_secs(8)), Duration::from_secs(1));
    }
}
