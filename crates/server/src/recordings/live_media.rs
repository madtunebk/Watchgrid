//! Serving a recording that is still being written: the partial file up to
//! what is on disk, its `mdat` size patched, and a freshly built `moov`
//! appended — a complete MP4 the browser can play and seek, with HTTP range
//! support.

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::recorder::live::Snapshot;

const CHUNK: u64 = 256 * 1024;

/// `(first, last)` byte of a single `Range: bytes=…` request, if valid.
fn parse_range(headers: &HeaderMap, len: u64) -> Option<(u64, u64)> {
    let spec = headers.get(header::RANGE)?.to_str().ok()?.strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    let (first, last) = match (a.trim(), b.trim()) {
        ("", n) => (len.checked_sub(n.parse().ok()?)?, len - 1),
        (a, "") => (a.parse().ok()?, len - 1),
        (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(len - 1)),
    };
    (first <= last && last < len).then_some((first, last))
}

pub async fn serve(snap: Snapshot, headers: &HeaderMap) -> Response {
    let len = snap.len();
    let (first, last, status) = match headers.get(header::RANGE) {
        None => (0, len - 1, StatusCode::OK),
        Some(_) => match parse_range(headers, len) {
            Some((a, b)) => (a, b, StatusCode::PARTIAL_CONTENT),
            None => {
                let mut resp = StatusCode::RANGE_NOT_SATISFIABLE.into_response();
                resp.headers_mut().insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{len}")).expect("ascii"));
                return resp;
            }
        },
    };
    let body = Body::from_stream(chunks(std::sync::Arc::new(snap), first, last));
    let mut resp = (status, body).into_response();
    let h = resp.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(last - first + 1));
    if status == StatusCode::PARTIAL_CONTENT {
        h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {first}-{last}/{len}")).expect("ascii"));
    }
    resp
}

/// Bytes `[first, last]` of the virtual file, a chunk at a time.
fn chunks(snap: std::sync::Arc<Snapshot>, first: u64, last: u64) -> impl futures::Stream<Item = std::io::Result<Bytes>> {
    futures::stream::unfold((first, None::<tokio::fs::File>), move |(at, mut file)| {
        let snap = snap.clone();
        async move {
            if at > last {
                return None;
            }
            let end = (at + CHUNK - 1).min(last);
            let chunk = if at < snap.data_end {
                // From the file, with the mdat size field filled in.
                let to = end.min(snap.data_end - 1);
                if file.is_none() {
                    match tokio::fs::File::open(&snap.path).await {
                        Ok(f) => file = Some(f),
                        Err(e) => return Some((Err(e), (last + 1, None))),
                    }
                }
                let f = file.as_mut().expect("opened");
                let mut buf = vec![0u8; (to - at + 1) as usize];
                let read = async {
                    f.seek(std::io::SeekFrom::Start(at)).await?;
                    f.read_exact(&mut buf).await
                };
                if let Err(e) = read.await {
                    return Some((Err(e), (last + 1, None)));
                }
                patch(&mut buf, at, snap.mdat_size_at, &snap.mdat_size.to_be_bytes());
                buf
            } else {
                let from = (at - snap.data_end) as usize;
                let to = (end - snap.data_end) as usize;
                snap.moov[from..=to].to_vec()
            };
            let next = at + chunk.len() as u64;
            Some((Ok(Bytes::from(chunk)), (next, file)))
        }
    })
}

/// Overwrite the part of `buf` (file bytes from `at`) that covers `value`
/// at file offset `field_at`.
fn patch(buf: &mut [u8], at: u64, field_at: u64, value: &[u8]) {
    for (i, b) in value.iter().enumerate() {
        let pos = field_at + i as u64;
        if pos >= at && pos < at + buf.len() as u64 {
            buf[(pos - at) as usize] = *b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_parse_like_browsers_send_them() {
        let h = |v: &str| {
            let mut m = HeaderMap::new();
            m.insert(header::RANGE, v.parse().unwrap());
            m
        };
        assert_eq!(parse_range(&h("bytes=0-"), 100), Some((0, 99)));
        assert_eq!(parse_range(&h("bytes=10-19"), 100), Some((10, 19)));
        assert_eq!(parse_range(&h("bytes=90-500"), 100), Some((90, 99)));
        assert_eq!(parse_range(&h("bytes=-10"), 100), Some((90, 99)));
        assert_eq!(parse_range(&h("bytes=100-"), 100), None);
        assert_eq!(parse_range(&h("items=0-1"), 100), None);
    }

    #[test]
    fn the_size_field_is_patched_across_chunk_edges() {
        let mut buf = vec![0u8; 4];
        patch(&mut buf, 10, 12, &[1, 2, 3, 4]);
        assert_eq!(buf, [0, 0, 1, 2], "only the part inside this chunk");
    }

    /// The whole virtual file, as the endpoint would stream it.
    async fn assemble(snap: Snapshot) -> Vec<u8> {
        use futures::StreamExt;
        let len = snap.len();
        let parts: Vec<_> = chunks(std::sync::Arc::new(snap), 0, len - 1).collect().await;
        parts.into_iter().flat_map(|p| p.unwrap().to_vec()).collect()
    }

    #[tokio::test]
    async fn a_clip_in_progress_is_a_complete_mp4() {
        use crate::media::audio::{AudioFrame, AudioTrack};
        use crate::media::{Frame, VideoTrack};
        use crate::recorder::live::LiveClip;
        use crate::recorder::writer::Mp4Writer;

        let path = std::env::temp_dir().join(format!("wg-live-clip-{}.mp4", std::process::id()));
        let audio = AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 };
        let mut w = Mp4Writer::create(path.clone(), Some(audio)).await.unwrap();
        let video = VideoTrack { width: 640, height: 360, avcc: vec![1, 0x64, 0, 0x1e, 0xff, 0xe0, 0] };
        let clip = LiveClip {
            recording_id: "rec-x".into(),
            camera_id: "cam".into(),
            reason: watchgrid_model::RecordingReason::Manual,
            path: path.clone(),
            mdat_size_at: w.layout().mdat_size_at,
            data_start: w.layout().data_start,
            video,
            index: w.index(),
            started_at: std::sync::Mutex::new(Some(chrono::Utc::now())),
        };
        assert!(clip.snapshot().is_none(), "nothing on disk yet");
        for i in 0..10i64 {
            w.push(Frame { pts: i * 3600, keyframe: i % 5 == 0, data: vec![0, 0, 0, 1, 0x41].into() }).await.unwrap();
            w.push_audio(&AudioFrame { pts: i * 3600, data: vec![0xfc, 1].into() }).await.unwrap();
        }
        w.flush().await.unwrap();
        let snap = clip.snapshot().expect("playable");
        let file = assemble(snap).await;
        let names: Vec<String> = crate::media::boxes::test_util::top_level(&file).into_iter().map(|b| b.0).collect();
        assert_eq!(names, ["ftyp", "mdat", "moov"], "the mdat size was patched so the boxes tile");
        assert_eq!(file.windows(4).filter(|w| *w == b"trak").count(), 2);
        assert_eq!(clip.recording().unwrap().end_time, None);
        let _ = std::fs::remove_file(path);
    }

    /// Half of a real `live-dump` written as a recording in progress, then
    /// served: `WG_LIVE_DUMP=in.mp4 WG_REC_OUT=out.mp4 cargo test -- --ignored live_clip_from_dump`
    #[tokio::test]
    #[ignore]
    async fn live_clip_from_dump() {
        use crate::media::audio::{AudioFrame, AudioTrack};
        use crate::media::{Frame, VideoTrack};
        use crate::recorder::live::LiveClip;
        use crate::recorder::writer::Mp4Writer;
        let (Ok(input), Ok(out)) = (std::env::var("WG_LIVE_DUMP"), std::env::var("WG_REC_OUT")) else { return };
        let buf = std::fs::read(input).unwrap();
        let u32_at = |i: usize| u32::from_be_bytes(buf[i..i + 4].try_into().unwrap());
        let (mut i, mut moof, mut samples) = (0usize, 0usize, Vec::new());
        while i + 8 <= buf.len() {
            let (size, kind) = (u32_at(i) as usize, &buf[i + 4..i + 8]);
            if kind == b"moof" {
                moof = i;
            } else if kind == b"mdat" {
                let find = |tag: &[u8]| moof + buf[moof..i].windows(4).position(|w| w == tag).unwrap() - 4;
                let (tfhd, tfdt, trun) = (find(b"tfhd"), find(b"tfdt"), find(b"trun"));
                let time = u64::from_be_bytes(buf[tfdt + 12..tfdt + 20].try_into().unwrap());
                samples.push((u32_at(tfhd + 12), time, u32_at(trun + 28) & 0x0100_0000 == 0, buf[i + 8..i + size].to_vec()));
            }
            i += size;
        }
        let avcc_at = buf.windows(4).position(|w| w == b"avcC").unwrap() - 4;
        let video = VideoTrack { width: 640, height: 360, avcc: buf[avcc_at + 8..avcc_at + u32_at(avcc_at) as usize].to_vec() };
        let partial = std::path::PathBuf::from(format!("{out}.partial"));
        let mut w = Mp4Writer::create(partial.clone(), Some(AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 })).await.unwrap();
        let clip = LiveClip {
            recording_id: "rec".into(),
            camera_id: "cam".into(),
            reason: watchgrid_model::RecordingReason::Manual,
            path: partial.clone(),
            mdat_size_at: w.layout().mdat_size_at,
            data_start: w.layout().data_start,
            video,
            index: w.index(),
            started_at: std::sync::Mutex::new(Some(chrono::Utc::now())),
        };
        let half = samples.len() / 2;
        for (t, time, key, data) in samples.into_iter().take(half) {
            if t == 1 {
                w.push(Frame { pts: time as i64, keyframe: key, data: data.into() }).await.unwrap();
            } else {
                w.push_audio(&AudioFrame { pts: (time * 90_000 / 48_000) as i64, data: data.into() }).await.unwrap();
            }
        }
        w.flush().await.unwrap();
        std::fs::write(&out, assemble(clip.snapshot().unwrap()).await).unwrap();
        println!("wrote {out}; recording reports {} s", clip.recording().unwrap().duration);
    }
}
