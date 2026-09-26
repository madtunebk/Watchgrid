//! Progressive (non-fragmented) MP4 files for recordings.
//!
//! Layout: `ftyp`, then `mdat` with samples appended as they arrive, then
//! `moov` written once at the end. Until `moov` exists the file is not
//! playable, which is why recordings are written as incomplete files and
//! only published after finalizing. The sample tables are kept in memory
//! (~16 bytes per sample). Audio, when recorded, is a second track whose
//! samples are interleaved with the video in the same `mdat`.

use super::audio::AudioTrack;
use super::boxes::{self, Media, Trak, VideoTrack, Writer};

/// `mdat` header: 32-bit size of 1, type, 64-bit "largesize".
const MDAT_HEADER: u64 = 16;

/// Bytes that start a recording file.
pub struct FileStart {
    pub bytes: Vec<u8>,
    /// Where the `mdat` largesize field sits (patched at finalize).
    pub mdat_size_at: u64,
    /// File offset of the first sample.
    pub data_start: u64,
}

pub fn file_start() -> FileStart {
    let mut w = Writer::new();
    boxes::ftyp(&mut w);
    let mdat_at = w.len() as u64;
    w.u32(1).bytes(b"mdat").u64(0);
    FileStart { mdat_size_at: mdat_at + 8, data_start: mdat_at + MDAT_HEADER, bytes: w.0 }
}

/// Value for the `mdat` largesize field once `data_len` sample bytes were written.
pub fn mdat_size(data_len: u64) -> u64 {
    MDAT_HEADER + data_len
}

#[derive(Debug, Default)]
pub struct SampleTable {
    offsets: Vec<u64>,
    sizes: Vec<u32>,
    durations: Vec<u32>,
    /// 1-based numbers of keyframes.
    sync: Vec<u32>,
    total: u64,
    /// Every sample is a sync sample (audio): no `stss`.
    all_sync: bool,
}

impl SampleTable {
    /// For an audio track: every sample decodes on its own.
    pub fn audio() -> Self {
        Self { all_sync: true, ..Self::default() }
    }

    pub fn push(&mut self, offset: u64, size: u32, duration: u32, keyframe: bool) {
        self.offsets.push(offset);
        self.sizes.push(size);
        self.durations.push(duration);
        if keyframe {
            self.sync.push(self.sizes.len() as u32);
        }
        self.total += u64::from(duration);
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    /// The samples lying completely before byte `end` (what is on disk of
    /// a file still being written).
    pub fn within(&self, end: u64) -> Self {
        let n = self.offsets.iter().zip(&self.sizes).take_while(|(o, s)| **o + u64::from(**s) <= end).count();
        Self {
            offsets: self.offsets[..n].to_vec(),
            sizes: self.sizes[..n].to_vec(),
            durations: self.durations[..n].to_vec(),
            sync: self.sync.iter().copied().filter(|k| *k as usize <= n).collect(),
            total: self.durations[..n].iter().map(|d| u64::from(*d)).sum(),
            all_sync: self.all_sync,
        }
    }

    /// Total duration in the track's timescale.
    pub fn duration(&self) -> u64 {
        self.total
    }

    fn write_tables(&self, w: &mut Writer) {
        let runs = runs(&self.durations);
        w.full(b"stts", 0, 0, |w| {
            w.u32(runs.len() as u32);
            for (count, duration) in &runs {
                w.u32(*count).u32(*duration);
            }
        });
        if !self.all_sync {
            w.full(b"stss", 0, 0, |w| {
                w.u32(self.sync.len() as u32);
                for n in &self.sync {
                    w.u32(*n);
                }
            });
        }
        // One sample per chunk: simple, and exact offsets for every frame.
        w.full(b"stsc", 0, 0, |w| {
            w.u32(1).u32(1).u32(1).u32(1);
        });
        w.full(b"stsz", 0, 0, |w| {
            w.u32(0).u32(self.sizes.len() as u32);
            for s in &self.sizes {
                w.u32(*s);
            }
        });
        w.full(b"co64", 0, 0, |w| {
            w.u32(self.offsets.len() as u32);
            for o in &self.offsets {
                w.u64(*o);
            }
        });
    }
}

/// The closing `moov` box: the video track and, if any audio was written,
/// the Opus track (durations in 48 kHz ticks).
pub fn moov(video: &VideoTrack, video_table: &SampleTable, audio: Option<(&AudioTrack, &SampleTable)>) -> Vec<u8> {
    let mut w = Writer::new();
    let video_tables = |w: &mut Writer| video_table.write_tables(w);
    let mut traks = vec![Trak { media: Media::Video(video), duration: video_table.total, sample_tables: Some(&video_tables) }];
    let audio_tables;
    if let Some((track, table)) = audio.filter(|(_, t)| t.len() > 0) {
        audio_tables = move |w: &mut Writer| table.write_tables(w);
        traks.push(Trak { media: Media::Audio(track), duration: table.total, sample_tables: Some(&audio_tables) });
    }
    boxes::moov(&mut w, &traks);
    w.0
}

/// Run-length encode durations as (count, duration).
fn runs(durations: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &d in durations {
        match out.last_mut() {
            Some((n, last)) if *last == d => *n += 1,
            _ => out.push((1, d)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::boxes::test_util::{find, top_level, u32_at};

    fn track() -> VideoTrack {
        VideoTrack { width: 640, height: 360, avcc: vec![1, 0x64, 0, 0x1e, 0xff, 0xe0, 0] }
    }

    /// Build a whole file in memory the way the recorder writes it.
    fn build(frames: &[(&[u8], u32, bool)]) -> Vec<u8> {
        let start = file_start();
        let mut file = start.bytes.clone();
        let mut table = SampleTable::default();
        for (data, duration, key) in frames {
            table.push(file.len() as u64, data.len() as u32, *duration, *key);
            file.extend_from_slice(data);
        }
        let data_len = file.len() as u64 - start.data_start;
        let at = start.mdat_size_at as usize;
        file[at..at + 8].copy_from_slice(&mdat_size(data_len).to_be_bytes());
        file.extend(moov(&track(), &table, None));
        file
    }

    #[test]
    fn file_is_ftyp_mdat_moov_and_offsets_hit_samples() {
        let frames: [(&[u8], u32, bool); 3] = [(&[0, 0, 0, 1, 0x65], 3600, true), (&[0, 0, 0, 2, 0x41, 9], 3600, false), (&[0, 0, 0, 1, 0x41], 3000, false)];
        let file = build(&frames);
        let names: Vec<String> = top_level(&file).into_iter().map(|b| b.0).collect();
        assert_eq!(names, ["ftyp", "mdat", "moov"]);

        let co64 = find(&file, b"co64");
        assert_eq!(u32_at(&file, co64 + 12), 3);
        for (i, (data, _, _)) in frames.iter().enumerate() {
            let at = co64 + 16 + i * 8;
            let offset = u64::from_be_bytes(file[at..at + 8].try_into().unwrap()) as usize;
            assert_eq!(&file[offset..offset + data.len()], *data);
        }
        let stss = find(&file, b"stss");
        assert_eq!((u32_at(&file, stss + 12), u32_at(&file, stss + 16)), (1, 1), "only the first frame is a keyframe");
        let mdhd = find(&file, b"mdhd");
        assert_eq!(u32_at(&file, mdhd + 24), 10_200, "media duration is the sum");
        let mvhd = find(&file, b"mvhd");
        assert_eq!(u32_at(&file, mvhd + 24), 113, "movie duration in ms");
    }

    #[test]
    fn durations_are_run_length_encoded() {
        assert_eq!(runs(&[3600, 3600, 3000, 3600]), vec![(2, 3600), (1, 3000), (1, 3600)]);
        assert!(runs(&[]).is_empty());
    }
}
