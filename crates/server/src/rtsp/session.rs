//! Opening a camera stream: DESCRIBE → SETUP (video; TCP or UDP from
//! Settings → Advanced) → PLAY.
//! Shared by the probe and the camera supervisor.

use std::sync::Arc;
use std::time::{Duration, Instant};

use retina::client::{
    Credentials, Demuxed, InitialSequenceNumberPolicy, PlayOptions, Session, SessionGroup, SessionOptions, SetupOptions,
    TcpTransportOptions, Transport, UdpTransportOptions,
};
use retina::codec::ParametersRef;

use crate::media::audio::Source as AudioSource;

/// Static facts about an opened stream.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamFacts {
    pub video_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub audio_codec: Option<String>,
}

pub struct Opened {
    pub facts: StreamFacts,
    /// Index of the video stream within the session.
    pub video: usize,
    /// Audio stream set up alongside, if the camera has one we can use:
    /// (index, codec, RTP clock rate).
    pub audio: Option<(usize, AudioSource, u32)>,
    pub stream: Demuxed,
    pub latency: Duration,
}

/// Fill width/height from the stream parameters when they become known.
pub fn refresh_dimensions(facts: &mut StreamFacts, stream: &Demuxed, video: usize) {
    if let Some(ParametersRef::Video(v)) = stream.streams()[video].parameters() {
        let (w, h) = v.pixel_dimensions();
        facts.width = Some(w);
        facts.height = Some(h);
    }
}

fn transport() -> Transport {
    if crate::settings::applied::rtsp_udp() { Transport::Udp(UdpTransportOptions::default()) } else { Transport::Tcp(TcpTransportOptions::default()) }
}

/// Open the camera's stream; with `audio`, also its G.711 audio.
pub async fn open(url: &str, username: &str, password: Option<&str>, audio: bool) -> Result<Opened, String> {
    let url = url::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;
    let creds = password.map(|p| Credentials { username: username.to_string(), password: p.to_string() });
    let options = SessionOptions::default()
        .creds(creds)
        .user_agent("Watchgrid".to_string())
        .session_group(Arc::new(SessionGroup::default()));

    let started = Instant::now();
    let mut session = Session::describe(url, options).await.map_err(|e| format!("DESCRIBE failed: {e}"))?;

    let mut facts = StreamFacts::default();
    let mut video = None;
    for (i, s) in session.streams().iter().enumerate() {
        match s.media() {
            "video" if video.is_none() => {
                video = Some(i);
                facts.video_codec = Some(s.encoding_name().to_uppercase());
                if let Some(ParametersRef::Video(v)) = s.parameters() {
                    let (w, h) = v.pixel_dimensions();
                    facts.width = Some(w);
                    facts.height = Some(h);
                }
            }
            "audio" if facts.audio_codec.is_none() => facts.audio_codec = Some(s.encoding_name().to_uppercase()),
            _ => {}
        }
    }
    let video = video.ok_or("the stream has no video track")?;
    session
        .setup(video, SetupOptions::default().transport(transport()).frame_format(retina::codec::FrameFormat::MP4))
        .await
        .map_err(|e| format!("SETUP failed: {e}"))?;
    let audio = if audio { setup_audio(&mut session).await } else { None };
    // Start RTP numbering from the first packet actually received: cameras
    // serving several clients (supervisor, tests, live view, recorder) often
    // announce a different sequence number in the PLAY response.
    let play = PlayOptions::default().initial_seq(InitialSequenceNumberPolicy::Ignore);
    let stream = session.play(play).await.map_err(|e| format!("PLAY failed: {e}"))?.demuxed().map_err(|e| format!("demux: {e}"))?;
    Ok(Opened { facts, video, audio, stream, latency: started.elapsed() })
}

/// Set up the camera's audio too when it is G.711 (the only kind Watchgrid
/// re-encodes). Audio is a bonus: any failure leaves a video-only session.
async fn setup_audio(session: &mut Session<retina::client::Described>) -> Option<(usize, AudioSource, u32)> {
    let (index, source, rate) = session.streams().iter().enumerate().find_map(|(i, s)| {
        (s.media() == "audio").then(|| AudioSource::from_encoding(s.encoding_name()).map(|src| (i, src, s.clock_rate_hz())))?
    })?;
    match session.setup(index, SetupOptions::default().transport(transport())).await {
        Ok(()) => Some((index, source, rate)),
        Err(e) => {
            tracing::debug!("audio SETUP failed, continuing without audio: {e}");
            None
        }
    }
}
