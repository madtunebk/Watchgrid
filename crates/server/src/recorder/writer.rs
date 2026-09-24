//! Writes frames into an incomplete MP4 file and finalizes it.

use std::io::{self, SeekFrom};
use std::path::{Path, PathBuf};

use tokio::fs::File;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};

use crate::media::mp4::{self, FileStart, SampleTable};
use crate::media::{Frame, SampleClock, VideoTrack};

const BUFFER: usize = 1 << 20;

pub struct Mp4Writer {
    path: PathBuf,
    file: BufWriter<File>,
    start: FileStart,
    /// Current end of file.
    position: u64,
    table: SampleTable,
    clock: SampleClock,
    /// Written once the next frame gives its duration.
    pending: Option<Frame>,
    /// Skip frames until a keyframe (at start and after lost frames).
    need_keyframe: bool,
}

pub struct Finished {
    pub path: PathBuf,
    pub size: u64,
    /// Timescale ticks.
    pub duration: u64,
    pub samples: usize,
}

impl Mp4Writer {
    pub async fn create(path: PathBuf) -> io::Result<Self> {
        let file = File::create(&path).await?;
        let mut file = BufWriter::with_capacity(BUFFER, file);
        let start = mp4::file_start();
        file.write_all(&start.bytes).await?;
        let position = start.bytes.len() as u64;
        Ok(Self { path, file, start, position, table: SampleTable::default(), clock: SampleClock::default(), pending: None, need_keyframe: true })
    }

    /// Accept a frame. Returns whether it was taken (false while waiting for a keyframe).
    pub async fn push(&mut self, frame: Frame) -> io::Result<bool> {
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
        self.pending = Some(frame);
        Ok(true)
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
        self.table.len() + usize::from(self.pending.is_some())
    }

    async fn write(&mut self, frame: Frame, duration: u32) -> io::Result<()> {
        self.file.write_all(&frame.data).await?;
        self.table.push(self.position, frame.data.len() as u32, duration, frame.keyframe);
        self.position += frame.data.len() as u64;
        Ok(())
    }

    /// Write the index and flush everything to disk. The file is playable afterwards.
    pub async fn finish(mut self, track: &VideoTrack) -> io::Result<Finished> {
        if let Some(prev) = self.pending.take() {
            self.write(prev, self.clock.last()).await?;
        }
        self.file.flush().await?;
        let mut file = self.file.into_inner();
        let data_len = self.position - self.start.data_start;
        file.seek(SeekFrom::Start(self.start.mdat_size_at)).await?;
        file.write_all(&mp4::mdat_size(data_len).to_be_bytes()).await?;
        file.seek(SeekFrom::End(0)).await?;
        let moov = self.table.moov(track);
        file.write_all(&moov).await?;
        file.sync_all().await?;
        Ok(Finished { path: self.path, size: self.position + moov.len() as u64, duration: self.table.duration(), samples: self.table.len() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
