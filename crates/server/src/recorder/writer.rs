//! Writes frames (and audio packets) into an incomplete MP4 file and
//! finalizes it. The sample index is shared ([`Index`]) so a clip can be
//! played while it is still being recorded.

use std::io::{self, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};

use crate::media::audio::{AudioFrame, AudioTrack, PACKET_DURATION};
use crate::media::mp4::{self, FileStart, SampleTable};
use crate::media::{Frame, SampleClock, TIMESCALE, VideoTrack};

const BUFFER: usize = 1 << 20;
/// One audio packet (20 ms) in 90 kHz ticks.
const PACKET_TICKS: i64 = TIMESCALE as i64 / 50;
/// An Opus packet with no audio data (TOC only, SILK narrowband 20 ms):
/// decoders play it as silence. Fills gaps so audio never drifts from video.
const SILENCE: [u8; 1] = [0x08];
/// Longer gaps (camera audio stopped) restart the audio timeline instead.
const MAX_FILL: i64 = 250;

/// Where every written sample is, and how much of the file is on disk.
#[derive(Debug, Default)]
pub struct Index {
    pub video: SampleTable,
    pub audio: Option<(AudioTrack, SampleTable)>,
    /// Bytes of the file flushed to disk (readable by others).
    pub flushed: u64,
}

pub struct Mp4Writer {
    path: PathBuf,
    file: BufWriter<File>,
    start: FileStart,
    /// Current end of file.
    position: u64,
    index: Arc<Mutex<Index>>,
    clock: SampleClock,
    /// Written once the next frame gives its duration.
    pending: Option<Frame>,
    /// Skip frames until a keyframe (at start and after lost frames).
    need_keyframe: bool,
    /// Newest accepted timestamp: frames at or before it are duplicates
    /// (pre-record buffer and live frames overlap).
    last_pts: Option<i64>,
    /// Camera time of the first video frame: audio starts there.
    video_start: Option<i64>,
    /// Camera time the next audio packet belongs at.
    audio_next: Option<i64>,
}

pub struct Finished {
    pub path: PathBuf,
    pub size: u64,
    /// Timescale ticks.
    pub duration: u64,
    pub samples: usize,
    /// Audio packets written (0: no audio track).
    pub audio_samples: usize,
}

impl Mp4Writer {
    /// `audio`: record this audio track alongside the video.
    pub async fn create(path: PathBuf, audio: Option<AudioTrack>) -> io::Result<Self> {
        let file = File::create(&path).await?;
        let mut file = BufWriter::with_capacity(BUFFER, file);
        let start = mp4::file_start();
        file.write_all(&start.bytes).await?;
        let position = start.bytes.len() as u64;
        let index = Index { audio: audio.map(|track| (track, SampleTable::audio())), flushed: position, ..Index::default() };
        Ok(Self {
            path,
            file,
            start,
            position,
            index: Arc::new(Mutex::new(index)),
            clock: SampleClock::default(),
            pending: None,
            need_keyframe: true,
            last_pts: None,
            video_start: None,
            audio_next: None,
        })
    }

    /// The shared sample index (for playing the clip while it records).
    pub fn index(&self) -> Arc<Mutex<Index>> {
        self.index.clone()
    }

    /// Where the file layout starts (`mdat` header and first sample).
    pub fn layout(&self) -> &FileStart {
        &self.start
    }

    /// Push buffered bytes to disk so a live viewer can read them.
    pub async fn flush(&mut self) -> io::Result<()> {
        self.file.flush().await?;
        self.lock().flushed = self.position;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        self.index.lock().expect("recording index lock")
    }

    /// Accept a frame. Returns whether it was taken (false while waiting for a keyframe).
    pub async fn push(&mut self, frame: Frame) -> io::Result<bool> {
        if self.last_pts.is_some_and(|last| frame.pts <= last) {
            return Ok(false);
        }
        if self.need_keyframe {
            if !frame.keyframe {
                return Ok(false);
            }
            self.need_keyframe = false;
        }
        if let Some(prev) = self.pending.take() {
            let duration = self.clock.duration(prev.pts, frame.pts);
            self.write(prev, duration).await?;
        }
        self.video_start.get_or_insert(frame.pts);
        self.last_pts = Some(frame.pts);
        self.pending = Some(frame);
        Ok(true)
    }

    /// Accept an audio packet (ignored without an audio track, before the
    /// first video frame, or when already written).
    pub async fn push_audio(&mut self, packet: &AudioFrame) -> io::Result<()> {
        let Some(start) = self.video_start.filter(|_| self.lock().audio.is_some()) else { return Ok(()) };
        if packet.pts < start {
            return Ok(());
        }
        let expected = self.audio_next.unwrap_or(start);
        if packet.pts + PACKET_TICKS / 2 < expected {
            return Ok(()); // pre-record and live overlap
        }
        let missing = (packet.pts - expected + PACKET_TICKS / 2) / PACKET_TICKS;
        let fill = if missing > MAX_FILL { 0 } else { missing.max(0) };
        for _ in 0..fill {
            self.write_audio(&SILENCE).await?;
        }
        self.write_audio(&packet.data).await?;
        self.audio_next = Some(if missing > MAX_FILL { packet.pts } else { expected + fill * PACKET_TICKS } + PACKET_TICKS);
        Ok(())
    }

    async fn write_audio(&mut self, data: &[u8]) -> io::Result<()> {
        self.file.write_all(data).await?;
        let offset = self.position;
        if let Some((_, table)) = self.lock().audio.as_mut() {
            table.push(offset, data.len() as u32, u32::from(PACKET_DURATION), true);
        }
        self.position += data.len() as u64;
        Ok(())
    }

    /// Frames were lost: close the current run and restart at a keyframe.
    pub async fn resync(&mut self) -> io::Result<()> {
        if let Some(prev) = self.pending.take() {
            self.write(prev, self.clock.last()).await?;
        }
        self.need_keyframe = true;
        Ok(())
    }

    pub fn samples(&self) -> usize {
        self.lock().video.len() + usize::from(self.pending.is_some())
    }

    async fn write(&mut self, frame: Frame, duration: u32) -> io::Result<()> {
        self.file.write_all(&frame.data).await?;
        let offset = self.position;
        self.lock().video.push(offset, frame.data.len() as u32, duration, frame.keyframe);
        self.position += frame.data.len() as u64;
        Ok(())
    }

    /// Write the index and flush everything to disk. The file is playable
    /// afterwards, with the index in front of the data (see [`mp4`]); if
    /// that rewrite fails, the index goes at the end instead.
    pub async fn finish(mut self, track: &VideoTrack) -> io::Result<Finished> {
        if let Some(prev) = self.pending.take() {
            self.write(prev, self.clock.last()).await?;
        }
        self.file.flush().await?;
        let mut file = self.file.into_inner();
        let data_len = self.position - self.start.data_start;
        file.seek(SeekFrom::Start(self.start.mdat_size_at)).await?;
        file.write_all(&mp4::mdat_size(data_len).to_be_bytes()).await?;
        file.flush().await?;
        let (moov_at_end, moov_in_front, duration, samples, audio_samples) = {
            let index = self.index.lock().expect("recording index lock");
            let at_end = mp4::moov(track, &index.video, index.audio.as_ref().map(|(t, table)| (t, table)));
            let by = at_end.len() as u64;
            let audio = index.audio.as_ref().map(|(t, table)| (t, table.shifted(by)));
            let in_front = mp4::moov(track, &index.video.shifted(by), audio.as_ref().map(|(t, table)| (*t, table)));
            debug_assert_eq!(in_front.len(), at_end.len(), "co64 offsets keep the index size");
            (at_end, in_front, index.video.duration(), index.video.len(), index.audio.as_ref().map_or(0, |(_, t)| t.len()))
        };
        let size = self.position + moov_at_end.len() as u64;
        match faststart(&self.path, self.start.mdat_at(), self.position, &moov_in_front).await {
            Ok(()) => drop(file),
            Err(e) => {
                tracing::warn!(path = %self.path.display(), "cannot put the index first, appending it: {e}");
                file.seek(SeekFrom::End(0)).await?;
                file.write_all(&moov_at_end).await?;
                file.sync_all().await?;
            }
        }
        Ok(Finished { path: self.path, size, duration, samples, audio_samples })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Rewrite `path` as `ftyp`, `moov`, `mdat` (`mdat` starts at `mdat_at`,
/// the file ends at `end`), then replace the original in one rename.
async fn faststart(path: &Path, mdat_at: u64, end: u64, moov: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("faststart");
    let result = async {
        let mut source = File::open(path).await?;
        let mut out = BufWriter::with_capacity(BUFFER, File::create(&temp).await?);
        let mut ftyp = vec![0; mdat_at as usize];
        tokio::io::AsyncReadExt::read_exact(&mut source, &mut ftyp).await?;
        out.write_all(&ftyp).await?;
        out.write_all(moov).await?;
        let copied = tokio::io::copy(&mut tokio::io::AsyncReadExt::take(source, end - mdat_at), &mut out).await?;
        if copied != end - mdat_at {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the recording is shorter than written"));
        }
        out.flush().await?;
        out.into_inner().sync_all().await?;
        tokio::fs::rename(&temp, path).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts: i64, keyframe: bool) -> Frame {
        Frame { pts, keyframe, data: vec![0, 0, 0, 1, if keyframe { 0x65 } else { 0x41 }].into() }
    }

    #[tokio::test]
    async fn audio_starts_with_the_video_and_gaps_become_silence() {
        let path = std::env::temp_dir().join(format!("wg-writer-audio-{}.mp4", std::process::id()));
        let track = AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 };
        let mut w = Mp4Writer::create(path.clone(), Some(track)).await.unwrap();
        let packet = |pts: i64| AudioFrame { pts, data: vec![0xfc, 0xaa].into() };
        w.push_audio(&packet(0)).await.unwrap(); // before any video: dropped
        w.push(frame(9_000, true)).await.unwrap();
        w.push_audio(&packet(9_000 + 2 * PACKET_TICKS)).await.unwrap(); // 2 packets late: 2 silence first
        w.push_audio(&packet(9_000 + 2 * PACKET_TICKS)).await.unwrap(); // duplicate
        w.push_audio(&packet(9_000 + 3 * PACKET_TICKS)).await.unwrap();
        w.push(frame(12_600, false)).await.unwrap();
        let video = VideoTrack { codec: crate::media::VideoCodec::H264, width: 640, height: 360, decoder_config: vec![1, 0x64, 0, 0x1e, 0xff, 0xe0, 0] };
        let done = w.finish(&video).await.unwrap();
        assert_eq!((done.samples, done.audio_samples), (2, 4));
        let file = std::fs::read(&path).unwrap();
        assert_eq!(file.windows(4).filter(|w| *w == b"trak").count(), 2);
        assert_eq!(file.windows(4).filter(|w| *w == b"stss").count(), 1, "only video has sync tables");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn the_index_comes_first_and_its_offsets_hit_the_samples() {
        let path = std::env::temp_dir().join(format!("wg-writer-faststart-{}.mp4", std::process::id()));
        let track = AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 };
        let mut w = Mp4Writer::create(path.clone(), Some(track)).await.unwrap();
        let frames = [(0i64, true, vec![0, 0, 0, 1, 0x65, 1]), (3_600, false, vec![0, 0, 0, 2, 0x41, 2, 3]), (7_200, false, vec![0, 0, 0, 1, 0x41])];
        for (pts, key, data) in &frames {
            w.push(Frame { pts: *pts, keyframe: *key, data: data.clone().into() }).await.unwrap();
            w.push_audio(&AudioFrame { pts: *pts, data: vec![0xfc, 0xaa].into() }).await.unwrap();
        }
        let video = VideoTrack { codec: crate::media::VideoCodec::H264, width: 640, height: 360, decoder_config: vec![1, 0x64, 0, 0x1e, 0xff, 0xe0, 0] };
        let done = w.finish(&video).await.unwrap();
        let file = std::fs::read(&path).unwrap();
        assert_eq!(done.size, file.len() as u64);
        assert!(!path.with_extension("faststart").exists());

        let u32_at = |i: usize| u32::from_be_bytes(file[i..i + 4].try_into().unwrap());
        let (mut at, mut order) = (0, Vec::new());
        while at < file.len() {
            let size = match u32_at(at) {
                1 => u64::from_be_bytes(file[at + 8..at + 16].try_into().unwrap()) as usize,
                s => s as usize,
            };
            order.push(String::from_utf8_lossy(&file[at + 4..at + 8]).into_owned());
            at += size;
        }
        assert_eq!(order, ["ftyp", "moov", "mdat"]);
        assert_eq!(at, file.len(), "the mdat size still covers the data");

        // The first co64 is the video track's.
        let co64 = file.windows(4).position(|w| w == b"co64").unwrap() - 4;
        assert_eq!(u32_at(co64 + 12), 3);
        for (i, (_, _, data)) in frames.iter().enumerate() {
            let offset = u64::from_be_bytes(file[co64 + 16 + i * 8..co64 + 24 + i * 8].try_into().unwrap()) as usize;
            assert_eq!(&file[offset..offset + data.len()], &data[..]);
        }
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn without_audio_the_file_has_one_track() {
        let path = std::env::temp_dir().join(format!("wg-writer-video-{}.mp4", std::process::id()));
        let mut w = Mp4Writer::create(path.clone(), None).await.unwrap();
        w.push(frame(0, true)).await.unwrap();
        w.push_audio(&AudioFrame { pts: 0, data: vec![1].into() }).await.unwrap();
        let video = VideoTrack { codec: crate::media::VideoCodec::H264, width: 640, height: 360, decoder_config: vec![1, 0x64, 0, 0x1e, 0xff, 0xe0, 0] };
        let done = w.finish(&video).await.unwrap();
        assert_eq!(done.audio_samples, 0);
        assert_eq!(std::fs::read(&path).unwrap().windows(4).filter(|w| *w == b"trak").count(), 1);
        let _ = std::fs::remove_file(path);
    }

    /// Rebuild a recording from a `watchgrid live-dump` file (real camera
    /// video + Opus audio) to check it in players:
    /// `WG_LIVE_DUMP=in.mp4 WG_REC_OUT=out.mp4 cargo test -- --ignored rebuild_recording`
    #[tokio::test]
    #[ignore]
    async fn rebuild_recording_from_live_dump() {
        let (Ok(input), Ok(out)) = (std::env::var("WG_LIVE_DUMP"), std::env::var("WG_REC_OUT")) else { return };
        let buf = std::fs::read(input).unwrap();
        let u32_at = |i: usize| u32::from_be_bytes(buf[i..i + 4].try_into().unwrap());
        let (mut i, mut samples) = (0usize, Vec::new());
        let mut moof = 0;
        while i + 8 <= buf.len() {
            let (size, kind) = (u32_at(i) as usize, &buf[i + 4..i + 8]);
            if kind == b"moof" {
                moof = i;
            } else if kind == b"mdat" {
                let find = |tag: &[u8]| moof + buf[moof..i].windows(4).position(|w| w == tag).unwrap() - 4;
                let (tfhd, tfdt, trun) = (find(b"tfhd"), find(b"tfdt"), find(b"trun"));
                let track = u32_at(tfhd + 12);
                let time = u64::from_be_bytes(buf[tfdt + 12..tfdt + 20].try_into().unwrap());
                let flags = u32_at(trun + 28);
                samples.push((track, time, flags & 0x0100_0000 == 0, buf[i + 8..i + size].to_vec()));
            }
            i += size;
        }
        let track = AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 };
        let mut w = Mp4Writer::create(out.into(), Some(track)).await.unwrap();
        let mut video = None;
        for (t, time, key, data) in samples {
            if t == 1 {
                w.push(Frame { pts: time as i64, keyframe: key, data: data.into() }).await.unwrap();
            } else {
                // 48 kHz back to the 90 kHz camera clock.
                w.push_audio(&AudioFrame { pts: (time * 90_000 / 48_000) as i64, data: data.into() }).await.unwrap();
            }
            video.get_or_insert(());
        }
        // Video parameters from the dump's avcC.
        let avcc_at = buf.windows(4).position(|w| w == b"avcC").unwrap() - 4;
        let avcc = buf[avcc_at + 8..avcc_at + u32_at(avcc_at) as usize].to_vec();
        let done = w.finish(&VideoTrack { codec: crate::media::VideoCodec::H264, width: 1280, height: 720, decoder_config: avcc }).await.unwrap();
        println!("video {} audio {}", done.samples, done.audio_samples);
    }
}
