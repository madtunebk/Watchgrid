//! ONVIF event subscription (PullPoint): create, pull, renew, unsubscribe.

use chrono::{DateTime, Utc};
use url::Url;

use super::client::Client;
use super::xml::{self, Notification};

const EVENTS: &str = "http://www.onvif.org/ver10/events/wsdl";
const WSN: &str = "http://docs.oasis-open.org/wsn/b-2";
/// Subscription lifetime requested (renewed well before it ends). Long
/// enough that renewals are rare; short enough that one left behind by a
/// dropped connection frees its slot on the camera soon.
pub const LIFETIME_SECS: u32 = 300;
/// How long a PullMessages call may wait for events on the camera: long
/// polls mean a couple of requests a minute when nothing happens (Tapo
/// refuses clients that ask too often).
const PULL_WAIT: &str = "PT30S";
/// The HTTP call must outlast that wait.
const PULL_LIMIT: std::time::Duration = std::time::Duration::from_secs(40);

pub struct Subscription {
    client: Client,
    address: Url,
}

impl Subscription {
    /// Connect to the device, find its event service and subscribe.
    pub async fn create(device_url: &str, username: &str, password: Option<String>) -> Result<Self, String> {
        let device = Url::parse(device_url.trim()).map_err(|_| "invalid ONVIF URL".to_string())?;
        let mut client = Client::new(device.clone(), username.to_string(), password);
        client.sync_clock().await;
        let events = super::probe::events_service(&client, &device).await?.ok_or("the camera has no ONVIF event service")?;
        let reply = client
            .call(
                &events,
                &format!("{EVENTS}/EventPortType/CreatePullPointSubscriptionRequest"),
                &format!(r#"<tev:CreatePullPointSubscription xmlns:tev="{EVENTS}"><tev:InitialTerminationTime>PT{LIFETIME_SECS}S</tev:InitialTerminationTime></tev:CreatePullPointSubscription>"#),
            )
            .await?;
        let address = xml::text(&reply, "Address").and_then(|a| Url::parse(&a).ok()).ok_or("the camera returned no subscription address")?;
        let address = super::probe::rebase(&address, &device);
        Ok(Self { client, address })
    }

    /// Wait (up to 30 s) for events.
    pub async fn pull(&self) -> Result<Vec<Notification>, String> {
        let reply = self
            .client
            .call_addressed_within(
                &self.address,
                &format!("{EVENTS}/PullPointSubscription/PullMessagesRequest"),
                &format!(r#"<tev:PullMessages xmlns:tev="{EVENTS}"><tev:Timeout>{PULL_WAIT}</tev:Timeout><tev:MessageLimit>64</tev:MessageLimit></tev:PullMessages>"#),
                PULL_LIMIT,
            )
            .await?;
        Ok(xml::notifications(&reply))
    }

    pub async fn renew(&self) -> Result<(), String> {
        self.client
            .call_addressed(
                &self.address,
                "http://docs.oasis-open.org/wsn/bw-2/SubscriptionManager/RenewRequest",
                &format!(r#"<wsnt:Renew xmlns:wsnt="{WSN}"><wsnt:TerminationTime>PT{LIFETIME_SECS}S</wsnt:TerminationTime></wsnt:Renew>"#),
            )
            .await
            .map(|_| ())
    }

    /// Best effort: the subscription also expires on its own.
    pub async fn unsubscribe(&self) {
        let _ = self
            .client
            .call_addressed(&self.address, "http://docs.oasis-open.org/wsn/bw-2/SubscriptionManager/UnsubscribeRequest", &format!(r#"<wsnt:Unsubscribe xmlns:wsnt="{WSN}"/>"#))
            .await;
    }
}

/// Beyond this, the camera's clock is wrong (no NTP without internet,
/// 1970 after a reboot) and its time would misplace events and their clips.
const MAX_CLOCK_SKEW: chrono::TimeDelta = chrono::TimeDelta::seconds(30);

/// Event time as reported by the camera, unless its clock is off; else now.
pub fn when(n: &Notification) -> DateTime<Utc> {
    trusted_time(n.time, Utc::now())
}

fn trusted_time(camera: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    camera.filter(|t| (*t - now).abs() <= MAX_CLOCK_SKEW).unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_camera_clock_is_not_trusted() {
        let now: DateTime<Utc> = "2026-09-25T18:00:00Z".parse().unwrap();
        let close = now - chrono::TimeDelta::seconds(2);
        assert_eq!(trusted_time(Some(close), now), close, "a synced camera keeps its exact time");
        assert_eq!(trusted_time(Some("1970-01-01T00:05:00Z".parse().unwrap()), now), now, "reset clock");
        assert_eq!(trusted_time(Some(now + chrono::TimeDelta::hours(3)), now), now, "wrong time zone / drift");
        assert_eq!(trusted_time(None, now), now);
    }
}
