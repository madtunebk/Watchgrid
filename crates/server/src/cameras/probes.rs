//! "Test connection" / "Test stream" from the camera form. They use the
//! same RTSP code as the supervisor, so a passing test means Watchgrid can
//! really connect. Stored passwords are used server-side only.

use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::time::timeout;
use watchgrid_model::{ConnectionProbe, ConnectionTest, OnvifConfig, OnvifProbe, StreamProbe, StreamTest};

use super::{service, url_credentials};
use crate::error::ApiResult;
use crate::rtsp::probe;
use crate::state::AppState;

const RTSP_PORT: u16 = 554;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_TIMEOUT: Duration = Duration::from_secs(12);
const MEASURE: Duration = Duration::from_secs(3);

/// First line only: some RTSP errors carry multi-line protocol dumps.
fn short(e: &str) -> String {
    e.lines().next().unwrap_or(e).trim().to_string()
}

/// Stored (username, password) of an existing camera.
async fn stored_login(state: &AppState, camera_id: Option<&str>) -> Option<(String, Option<String>)> {
    let id = camera_id?;
    service::connection_info(&state.db, &state.credentials, id).await.ok().flatten().map(|i| (i.username, i.password))
}

pub async fn connection(req: ConnectionTest) -> ApiResult<ConnectionProbe> {
    let fail = |message: String| Ok(ConnectionProbe { ok: false, message, latency_ms: None, device: None });
    let host = req.host.trim();
    if host.is_empty() {
        return fail("Enter a host or IP address first".into());
    }
    let started = Instant::now();
    match timeout(CONNECT_TIMEOUT, TcpStream::connect((host, RTSP_PORT))).await {
        Err(_) => fail(format!("No answer from {host}:{RTSP_PORT} within 5 s")),
        Ok(Err(e)) => fail(format!("Cannot reach {host}:{RTSP_PORT}: {e}")),
        Ok(Ok(_)) => Ok(ConnectionProbe {
            ok: true,
            message: format!("RTSP port {RTSP_PORT} is reachable. Use Test main stream to check the login."),
            latency_ms: Some(started.elapsed().as_millis() as u32),
            device: None,
        }),
    }
}

pub async fn stream(state: &AppState, req: StreamTest) -> ApiResult<StreamProbe> {
    let fail = |message: String| {
        Ok(StreamProbe { ok: false, message, codec: None, width: None, height: None, fps: None, audio_codec: None, latency_ms: None })
    };
    let split = url_credentials::split(&req.url);
    if !(split.url.starts_with("rtsp://") || split.url.starts_with("rtsps://")) {
        return fail("URL must start with rtsp:// or rtsps://".into());
    }
    // Precedence: what the form sends, then credentials in the URL, then
    // (for an existing camera) what is stored.
    let stored = stored_login(state, req.camera_id.as_deref()).await;
    let username = Some(req.username.trim().to_string())
        .filter(|u| !u.is_empty())
        .or(split.username)
        .or_else(|| stored.as_ref().map(|s| s.0.clone()))
        .unwrap_or_default();
    let password = req.password.filter(|p| !p.is_empty()).or(split.password).or_else(|| stored.and_then(|s| s.1));

    match timeout(STREAM_TIMEOUT, probe::probe(&split.url, &username, password.as_deref(), MEASURE)).await {
        Err(_) => fail("The camera did not deliver video within 12 s".into()),
        Ok(Err(e)) => fail(short(&e)),
        Ok(Ok(r)) => Ok(StreamProbe {
            ok: true,
            message: "Stream opened successfully".into(),
            codec: r.facts.video_codec.clone(),
            width: r.facts.width,
            height: r.facts.height,
            fps: Some(r.fps() as f32),
            audio_codec: r.facts.audio_codec.clone(),
            latency_ms: Some(r.connect_latency.as_millis() as u32),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_line_errors_are_shortened() {
        assert_eq!(short("RTSP framing error: Unexpected RTSP response Response {\n  status: 400\n}"), "RTSP framing error: Unexpected RTSP response Response {");
    }

    #[tokio::test]
    async fn empty_host_is_reported_not_attempted() {
        let r = connection(ConnectionTest { host: " ".into(), username: String::new(), password: None, camera_id: None }).await.unwrap();
        assert!(!r.ok);
    }

    #[tokio::test]
    async fn unreachable_host_fails_cleanly() {
        // TEST-NET-1 is never routed.
        let r = connection(ConnectionTest { host: "192.0.2.1".into(), username: String::new(), password: None, camera_id: None }).await.unwrap();
        assert!(!r.ok);
    }
}

/// ONVIF device and event check. A blank password falls back to the one
/// stored for the camera with this ONVIF URL.
pub async fn onvif(state: &AppState, config: OnvifConfig) -> ApiResult<OnvifProbe> {
    let url = config.url.trim().to_string();
    let mut username = config.username.trim().to_string();
    let mut password = config.password.filter(|p| !p.is_empty());
    if password.is_none()
        && let Some((stored_user, stored_pw)) = service::stored_onvif_login(state, &url).await?
    {
        if username.is_empty() {
            username = stored_user;
        }
        password = stored_pw;
    }
    Ok(crate::onvif::probe(&url, &username, password).await)
}
