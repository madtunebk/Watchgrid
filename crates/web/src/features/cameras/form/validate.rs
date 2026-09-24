//! Client-side validation. The backend validates again; this is for
//! immediate feedback.

use crate::api::CameraInput;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Errors {
    pub name: Option<String>,
    pub host: Option<String>,
    pub main_url: Option<String>,
    pub sub_url: Option<String>,
    pub onvif_url: Option<String>,
}

impl Errors {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn rtsp(url: &str) -> bool {
    url.starts_with("rtsp://") || url.starts_with("rtsps://")
}

pub fn check(i: &CameraInput) -> Errors {
    let mut e = Errors::default();
    if i.name.is_empty() {
        e.name = Some("Give the camera a name".into());
    }
    if i.host.is_empty() {
        e.host = Some("Host or IP address is required".into());
    } else if i.host.contains(char::is_whitespace) || i.host.contains("://") {
        e.host = Some("Enter only the host name or IP, e.g. 192.168.1.26".into());
    }
    if i.main_stream_url.is_empty() {
        e.main_url = Some("The main RTSP stream is required".into());
    } else if !rtsp(&i.main_stream_url) {
        e.main_url = Some("Must start with rtsp:// or rtsps://".into());
    }
    if let Some(sub) = &i.sub_stream_url
        && !rtsp(sub)
    {
        e.sub_url = Some("Must start with rtsp:// or rtsps:// (or leave empty)".into());
    }
    if let Some(o) = &i.onvif
        && !(o.url.starts_with("http://") || o.url.starts_with("https://"))
    {
        e.onvif_url = Some("ONVIF device service URL, e.g. http://192.168.1.26/onvif/device_service".into());
    }
    e
}
