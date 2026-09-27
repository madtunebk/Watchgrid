//! "Test ONVIF": who is this device, which events does it offer, and do
//! they actually arrive? (A camera can list motion topics and still drop
//! every pull, as the TC72 does.)

use url::Url;
use watchgrid_model::{EventDelivery, OnvifProbe};

use super::client::Client;
use super::pullpoint::Subscription;
use super::{topics, xml};

const DEVICE: &str = "http://www.onvif.org/ver10/device/wsdl";
const EVENTS: &str = "http://www.onvif.org/ver10/events/wsdl";

pub async fn probe(url: &str, username: &str, password: Option<String>) -> OnvifProbe {
    match run(url, username, password).await {
        Ok(p) => p,
        Err(message) => OnvifProbe { ok: false, message, event_topics: vec![], detections: vec![], delivery: None },
    }
}

async fn run(url: &str, username: &str, password: Option<String>) -> Result<OnvifProbe, String> {
    let url = Url::parse(url.trim()).map_err(|_| "enter the device service URL, e.g. http://192.168.1.20/onvif/device_service".to_string())?;
    let mut client = Client::new(url.clone(), username.to_string(), password.clone());
    client.sync_clock().await;

    let info = client.call(&url, &format!("{DEVICE}/GetDeviceInformation"), &format!(r#"<tds:GetDeviceInformation xmlns:tds="{DEVICE}"/>"#)).await?;
    let field = |name| xml::text(&info, name).filter(|s| !s.is_empty());
    let device = [field("Manufacturer"), field("Model")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let firmware = field("FirmwareVersion").map(|f| format!(", firmware {f}")).unwrap_or_default();

    let Some(events_url) = events_service(&client, &url).await? else {
        return Ok(OnvifProbe { ok: true, message: format!("{device}{firmware} — no ONVIF event service"), event_topics: vec![], detections: vec![], delivery: None });
    };

    let props = client.call(&events_url, &format!("{EVENTS}/EventPortType/GetEventPropertiesRequest"), &format!(r#"<tev:GetEventProperties xmlns:tev="{EVENTS}"/>"#)).await?;
    let event_topics = xml::topics(&props);
    let detections = topics::detections(&event_topics);
    let message = if event_topics.is_empty() { format!("{device}{firmware} — the camera lists no event topics") } else { format!("{device}{firmware} — {} event topics", event_topics.len()) };
    let delivery = Some(delivery(url.as_str(), username, password).await);
    Ok(OnvifProbe { ok: true, message, event_topics, detections, delivery })
}

/// Pulls that must succeed in a row (the first alone can pass by luck).
const PULLS: usize = 2;

/// Subscribe, pull a couple of times, unsubscribe: do events arrive?
async fn delivery(url: &str, username: &str, password: Option<String>) -> EventDelivery {
    let sub = match Subscription::create(url, username, password).await {
        Ok(s) => s,
        Err(e) => return EventDelivery { ok: false, message: format!("The camera refuses event subscriptions ({e})") },
    };
    let mut result = Ok(0);
    for _ in 0..PULLS {
        match sub.pull().await {
            Ok(list) => result = result.map(|n| n + list.len()),
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    sub.unsubscribe().await;
    match result {
        Ok(0) => EventDelivery { ok: true, message: "Events arrive (subscribed and pulled; nothing happened meanwhile)".into() },
        Ok(n) => EventDelivery { ok: true, message: format!("Events arrive ({n} received while testing)") },
        Err(e) => EventDelivery { ok: false, message: format!("Events don't arrive: {e}. Motion from this camera's events won't work; choose Software detection in the Motion tab.") },
    }
}

/// Address of the device's event service, if it has one.
pub async fn events_service(client: &Client, device: &Url) -> Result<Option<Url>, String> {
    let caps = client
        .call(device, &format!("{DEVICE}/GetCapabilities"), &format!(r#"<tds:GetCapabilities xmlns:tds="{DEVICE}"><tds:Category>Events</tds:Category></tds:GetCapabilities>"#))
        .await?;
    // Some devices advertise an address other than the one we reached them on.
    Ok(xml::xaddr_in(&caps, "Events").and_then(|a| Url::parse(&a).ok()).map(|u| rebase(&u, device)))
}

/// Use the host we can reach but keep the advertised port and path:
/// devices report their LAN address wrongly more often than their ports
/// (Tapo serves subscriptions on their own ports, e.g. :1026).
pub fn rebase(advertised: &Url, reached: &Url) -> Url {
    let mut u = advertised.clone();
    let _ = u.set_host(reached.host_str());
    u
}

#[cfg(test)]
mod tests {
    use super::rebase;
    use url::Url;

    #[test]
    fn advertised_addresses_use_the_reachable_host() {
        let adv = Url::parse("http://10.0.0.1:1026/event-1026_1026").unwrap();
        let reached = Url::parse("http://192.168.1.215:2020/onvif/device_service").unwrap();
        assert_eq!(rebase(&adv, &reached).as_str(), "http://192.168.1.215:1026/event-1026_1026");
    }
}
