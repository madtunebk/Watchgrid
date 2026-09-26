//! ONVIF PTZ: pan, tilt and zoom plus presets, for cameras that move.
//!
//! Moves are continuous with a short camera-side timeout: the browser keeps
//! repeating the move while a direction is held, so a lost "stop" (closed
//! tab, dropped network) never leaves the camera turning.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use url::Url;

use super::client::Client;
use super::probe::rebase;
use super::xml;

const DEVICE: &str = "http://www.onvif.org/ver10/device/wsdl";
const MEDIA: &str = "http://www.onvif.org/ver10/media/wsdl";
const PTZ: &str = "http://www.onvif.org/ver20/ptz/wsdl";
const SCHEMA: &str = "http://www.onvif.org/ver10/schema";
/// The camera stops by itself this long after the last move request.
const MOVE_TIMEOUT: &str = "PT1S";

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub token: String,
    pub name: String,
}

pub struct Ptz {
    client: Client,
    service: Url,
    profile: String,
}

impl Ptz {
    /// The camera's PTZ service, or `None` if it has none.
    pub async fn connect(device: &str, username: &str, password: Option<String>) -> Result<Option<Self>, String> {
        let device = Url::parse(device.trim()).map_err(|_| "invalid ONVIF address".to_string())?;
        let mut client = Client::new(device.clone(), username.to_string(), password);
        client.sync_clock().await;
        let caps = client
            .call(&device, &format!("{DEVICE}/GetCapabilities"), &format!(r#"<tds:GetCapabilities xmlns:tds="{DEVICE}"><tds:Category>All</tds:Category></tds:GetCapabilities>"#))
            .await?;
        let address = |section| xml::xaddr_in(&caps, section).and_then(|a| Url::parse(&a).ok()).map(|u| rebase(&u, &device));
        let (Some(service), Some(media)) = (address("PTZ"), address("Media")) else { return Ok(None) };
        let profiles = client.call(&media, &format!("{MEDIA}/GetProfiles"), &format!(r#"<trt:GetProfiles xmlns:trt="{MEDIA}"/>"#)).await?;
        let Some(profile) = ptz_profile(&profiles) else { return Ok(None) };
        Ok(Some(Self { client, service, profile }))
    }

    async fn call(&self, action: &str, body: &str) -> Result<String, String> {
        self.client.call(&self.service, &format!("{PTZ}/{action}"), body).await
    }

    /// Move at these speeds (-1…1; pan right, tilt up, zoom in are positive).
    pub async fn move_at(&self, pan: f32, tilt: f32, zoom: f32) -> Result<(), String> {
        let (pan, tilt, zoom) = (pan.clamp(-1.0, 1.0), tilt.clamp(-1.0, 1.0), zoom.clamp(-1.0, 1.0));
        // Cameras without zoom may reject a zoom velocity, even a zero one.
        let zoom = if zoom == 0.0 { String::new() } else { format!(r#"<tt:Zoom x="{zoom}"/>"#) };
        let body = format!(
            r#"<tptz:ContinuousMove xmlns:tptz="{PTZ}" xmlns:tt="{SCHEMA}"><tptz:ProfileToken>{}</tptz:ProfileToken><tptz:Velocity><tt:PanTilt x="{pan}" y="{tilt}"/>{zoom}</tptz:Velocity><tptz:Timeout>{MOVE_TIMEOUT}</tptz:Timeout></tptz:ContinuousMove>"#,
            xml::escape(&self.profile)
        );
        self.call("ContinuousMove", &body).await.map(|_| ())
    }

    pub async fn stop(&self) -> Result<(), String> {
        let body = format!(
            r#"<tptz:Stop xmlns:tptz="{PTZ}"><tptz:ProfileToken>{}</tptz:ProfileToken><tptz:PanTilt>true</tptz:PanTilt><tptz:Zoom>true</tptz:Zoom></tptz:Stop>"#,
            xml::escape(&self.profile)
        );
        self.call("Stop", &body).await.map(|_| ())
    }

    pub async fn presets(&self) -> Result<Vec<Preset>, String> {
        let body = format!(r#"<tptz:GetPresets xmlns:tptz="{PTZ}"><tptz:ProfileToken>{}</tptz:ProfileToken></tptz:GetPresets>"#, xml::escape(&self.profile));
        Ok(parse_presets(&self.call("GetPresets", &body).await?))
    }

    pub async fn goto_preset(&self, token: &str) -> Result<(), String> {
        let body = format!(
            r#"<tptz:GotoPreset xmlns:tptz="{PTZ}"><tptz:ProfileToken>{}</tptz:ProfileToken><tptz:PresetToken>{}</tptz:PresetToken></tptz:GotoPreset>"#,
            xml::escape(&self.profile),
            xml::escape(token)
        );
        self.call("GotoPreset", &body).await.map(|_| ())
    }

    /// Save the current position under `name`; returns the new preset.
    pub async fn save_preset(&self, name: &str) -> Result<Preset, String> {
        let body = format!(
            r#"<tptz:SetPreset xmlns:tptz="{PTZ}"><tptz:ProfileToken>{}</tptz:ProfileToken><tptz:PresetName>{}</tptz:PresetName></tptz:SetPreset>"#,
            xml::escape(&self.profile),
            xml::escape(name)
        );
        let reply = self.call("SetPreset", &body).await?;
        let token = xml::text(&reply, "PresetToken").filter(|t| !t.is_empty()).ok_or("the camera did not return the new preset")?;
        Ok(Preset { token, name: name.to_string() })
    }

    pub async fn remove_preset(&self, token: &str) -> Result<(), String> {
        let body = format!(
            r#"<tptz:RemovePreset xmlns:tptz="{PTZ}"><tptz:ProfileToken>{}</tptz:ProfileToken><tptz:PresetToken>{}</tptz:PresetToken></tptz:RemovePreset>"#,
            xml::escape(&self.profile),
            xml::escape(token)
        );
        self.call("RemovePreset", &body).await.map(|_| ())
    }
}

fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}

fn token_of(e: &BytesStart<'_>) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == b"token").and_then(|a| a.unescape_value().ok()).map(|v| v.into_owned())
}

/// The first media profile with a PTZ configuration (else the first one).
fn ptz_profile(xml: &str) -> Option<String> {
    let mut r = Reader::from_str(xml);
    let (mut first, mut current) = (None, None);
    loop {
        match r.read_event().ok()? {
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == b"Profiles" => {
                current = token_of(&e);
                first = first.or_else(|| current.clone());
            }
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == b"PTZConfiguration" && current.is_some() => return current,
            Event::Eof => return first,
            _ => {}
        }
    }
}

fn parse_presets(xml: &str) -> Vec<Preset> {
    let mut r = Reader::from_str(xml);
    let (mut out, mut token, mut in_name) = (Vec::new(), None::<String>, false);
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) if local(e.name().as_ref()) == b"Preset" => token = token_of(&e),
            Ok(Event::Start(e)) if token.is_some() && local(e.name().as_ref()) == b"Name" => in_name = true,
            Ok(Event::Text(t)) if in_name => {
                if let (Some(tok), Ok(name)) = (token.take(), t.decode()) {
                    out.push(Preset { token: tok, name: name.trim().to_string() });
                }
                in_name = false;
            }
            Ok(Event::End(e)) if local(e.name().as_ref()) == b"Preset" => {
                // A preset without a name: show its token.
                if let Some(tok) = token.take() {
                    out.push(Preset { name: tok.clone(), token: tok });
                }
            }
            Ok(Event::Eof) | Err(_) => return out,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_profile_with_ptz() {
        let xml = r#"<trt:GetProfilesResponse><trt:Profiles token="profile_1"><tt:Name>main</tt:Name></trt:Profiles>
            <trt:Profiles token="profile_2"><tt:PTZConfiguration token="ptz0"/></trt:Profiles></trt:GetProfilesResponse>"#;
        assert_eq!(ptz_profile(xml).as_deref(), Some("profile_2"));
        assert_eq!(ptz_profile(r#"<a><trt:Profiles token="only"/></a>"#).as_deref(), Some("only"));
    }

    #[test]
    fn reads_presets_with_and_without_names() {
        let xml = r#"<tptz:GetPresetsResponse><tptz:Preset token="1"><tt:Name>Door</tt:Name></tptz:Preset>
            <tptz:Preset token="2"></tptz:Preset></tptz:GetPresetsResponse>"#;
        assert_eq!(parse_presets(xml), [Preset { token: "1".into(), name: "Door".into() }, Preset { token: "2".into(), name: "2".into() }]);
    }
}
