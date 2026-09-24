//! Connect to one stream and measure a few seconds of media. Used by
//! `watchgrid probe` and the API's "Test stream".

use std::time::{Duration, Instant};

use futures::StreamExt;
use retina::codec::CodecItem;

use super::session::{self, StreamFacts};

#[derive(Debug, Default)]
pub struct Report {
    pub facts: StreamFacts,
    pub frames: u32,
    pub keyframes: u32,
    pub bytes: u64,
    pub measured: Duration,
    pub connect_latency: Duration,
}

impl Report {
    pub fn fps(&self) -> f64 {
        self.frames as f64 / self.measured.as_secs_f64().max(0.001)
    }

    pub fn kbps(&self) -> f64 {
        self.bytes as f64 * 8.0 / 1000.0 / self.measured.as_secs_f64().max(0.001)
    }
}

pub async fn probe(url: &str, username: &str, password: Option<&str>, measure_for: Duration) -> Result<Report, String> {
    let mut opened = session::open(url, username, password).await?;
    let mut report = Report { facts: opened.facts.clone(), connect_latency: opened.latency, ..Default::default() };

    let measuring = Instant::now();
    let deadline = tokio::time::sleep(measure_for);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            item = opened.stream.next() => match item {
                None => return Err("the camera closed the stream".into()),
                Some(Err(e)) => return Err(format!("stream error: {e}")),
                Some(Ok(CodecItem::VideoFrame(f))) => {
                    report.frames += 1;
                    report.bytes += f.data().len() as u64;
                    if f.is_random_access_point() {
                        report.keyframes += 1;
                    }
                    if report.facts.width.is_none() {
                        session::refresh_dimensions(&mut report.facts, &opened.stream, opened.video);
                    }
                }
                Some(Ok(_)) => {}
            }
        }
    }
    report.measured = measuring.elapsed();
    Ok(report)
}
