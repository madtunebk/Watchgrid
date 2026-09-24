//! "Test ONVIF": who is this device and which events does it offer?

use url::Url;
use watchgrid_model::OnvifProbe;

use super::client::Client;
use super::{topics, xml};

const DEVICE: &str = "http://www.onvif.org/ver10/device/wsdl";
const EVENTS: &str = "http://www.onvif.org/ver10/events/wsdl";

pub async fn probe(url: &str, username: &str, password: Option<String>) -> OnvifProbe {
    match run(url, username, password).await {
        Ok(p) => p,
        Err(message) => OnvifProbe { ok: false, message, event_topics: vec![], detections: vec![] },
    }
}

async fn run(url: &str, username: &str, password: Option<String>) -> Result<OnvifProbe, String> {
    let url = Url::parse(url.trim()).map_err(|_| "enter the device service URL, e.g. http://192.168.1.20/onvif/device_service".to_string())?;
    let mut client = Client::new(url.clone(), username.to_string(), password);
    client.sync_clock().await;

    let info = client.call(&url, &format!("{DEVICE}/GetDeviceInformation"), &format!(r#"<tds:GetDeviceInformation xmlns:tds="{DEVICE}"/>"#)).await?;
    let field = |name| xml::text(&info, name).filter(|s| !s.is_empty());
    let device = [field("Manufacturer"), field("Model")].into_iter().flatten().collect::<Vec<_>>().join(" ");
    let firmware = field("FirmwareVersion").map(|f| format!(", firmware {f}")).unwrap_or_default();

    let caps = client
        .call(&url, &format!("{DEVICE}/GetCapabilities"), &format!(r#"<tds:GetCapabilities xmlns:tds="{DEVICE}"><tds:Category>Events</tds:Category></tds:GetCapabilities>"#))
        .await?;
    let Some(events_url) = xml::xaddr_in(&caps, "Events").and_then(|a| Url::parse(&a).ok()) else {
        return Ok(OnvifProbe { ok: true, message: format!("{device}{firmware} — no ONVIF event service"), event_topics: vec![], detections: vec![] });
    };
    // Some devices advertise an address other than the one we reached them on.
    let events_url = rebase(&events_url, &url);

    let props = client.call(&events_url, &format!("{EVENTS}/EventPortType/GetEventPropertiesRequest"), &format!(r#"<tev:GetEventProperties xmlns:tev="{EVENTS}"/>"#)).await?;
    let event_topics = xml::topics(&props);
    let detections = topics::detections(&event_topics);
    let message = if event_topics.is_empty() { format!("{device}{firmware} — the camera lists no event topics") } else { format!("{device}{firmware} — {} event topics", event_topics.len()) };
    Ok(OnvifProbe { ok: true, message, event_topics, detections })
}

/// Keep the advertised path but use the host and port we can reach.
fn rebase(advertised: &Url, reached: &Url) -> Url {
    let mut u = advertised.clone();
    let _ = u.set_host(reached.host_str());
    let _ = u.set_port(reached.port());
    u
}

#[cfg(test)]
mod tests {
    use super::rebase;
    use url::Url;

    #[test]
    fn advertised_addresses_use_the_reachable_host() {
        let adv = Url::parse("http://10.0.0.1:8000/onvif/Events").unwrap();
        let reached = Url::parse("http://192.168.1.20/onvif/device_service").unwrap();
        assert_eq!(rebase(&adv, &reached).as_str(), "http://192.168.1.20/onvif/Events");
    }
}
