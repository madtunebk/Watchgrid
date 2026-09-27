//! Mock pre-save tests. Deterministic rules so every result can be shown:
//! - a host ending in ".31" (the offline Garage) or containing "fail" times out
//! - a URL that is not rtsp:// or rtsps:// is rejected
//! - ONVIF needs an http(s):// device-service URL

use gloo_timers::future::TimeoutFuture;

use crate::api::{ApiResult, ConnectionProbe, ConnectionTest, EventType, OnvifConfig, OnvifProbe, StreamProbe, StreamTest, EventDelivery};

fn unreachable(host: &str) -> bool {
    host.ends_with(".31") || host.contains("fail")
}

fn host_of(url: &str) -> &str {
    let rest = url.split("://").nth(1).unwrap_or("");
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    rest.split([':', '/']).next().unwrap_or("")
}

pub async fn connection(req: ConnectionTest) -> ApiResult<ConnectionProbe> {
    let (host, username) = (req.host.as_str(), req.username.as_str());
    TimeoutFuture::new(900).await;
    let fail = |message: &str| Ok(ConnectionProbe { ok: false, message: message.into(), latency_ms: None, device: None });
    if host.trim().is_empty() {
        return fail("Enter a host or IP address first");
    }
    if unreachable(host) {
        return fail("No response from host (timeout after 5 s)");
    }
    if username.trim().is_empty() {
        return fail("Host reachable, but authentication failed (no username)");
    }
    Ok(ConnectionProbe { ok: true, message: "Connection successful".into(), latency_ms: Some(12), device: Some("Hikvision DS-2CD2143G2-I".into()) })
}

pub async fn stream(req: StreamTest) -> ApiResult<StreamProbe> {
    let url = req.url.as_str();
    TimeoutFuture::new(1400).await;
    let fail = |message: String| {
        Ok(StreamProbe { ok: false, message, codec: None, width: None, height: None, fps: None, audio_codec: None, latency_ms: None })
    };
    let url = url.trim();
    if url.is_empty() {
        return fail("Enter an RTSP URL first".into());
    }
    if !(url.starts_with("rtsp://") || url.starts_with("rtsps://")) {
        return fail("URL must start with rtsp:// or rtsps://".into());
    }
    if unreachable(host_of(url)) {
        return fail(format!("Could not open stream: connection to {} timed out", host_of(url)));
    }
    // Paths ending in 2 look like substreams in most camera firmwares.
    let sub = url.trim_end_matches('/').ends_with('2');
    Ok(StreamProbe {
        ok: true,
        message: "Stream opened successfully".into(),
        codec: Some(if sub { "H.264" } else { "H.265" }.into()),
        width: Some(if sub { 640 } else { 1920 }),
        height: Some(if sub { 360 } else { 1080 }),
        fps: Some(if sub { 15.0 } else { 25.0 }),
        audio_codec: (!sub).then(|| "AAC".into()),
        latency_ms: Some(if sub { 61 } else { 84 }),
    })
}

pub async fn onvif(config: &OnvifConfig) -> ApiResult<OnvifProbe> {
    TimeoutFuture::new(1100).await;
    let fail = |message: String| Ok(OnvifProbe { ok: false, message, event_topics: vec![], detections: vec![], delivery: None });
    let url = config.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return fail("ONVIF URL must start with http:// or https://".into());
    }
    if unreachable(host_of(url)) {
        return fail("ONVIF device service did not respond".into());
    }
    Ok(OnvifProbe {
        ok: true,
        message: "ONVIF events available (PullPoint subscription)".into(),
        event_topics: vec![
            "tns1:RuleEngine/CellMotionDetector/Motion".into(),
            "tns1:RuleEngine/MyRuleDetector/PeopleDetect".into(),
            "tns1:RuleEngine/MyRuleDetector/VehicleDetect".into(),
            "tns1:VideoSource/MotionAlarm".into(),
        ],
        detections: vec![EventType::Motion, EventType::Person, EventType::Vehicle],
        delivery: Some(EventDelivery { ok: true, message: "Events arrive (subscribed and pulled; nothing happened meanwhile)".into() }),
    })
}
