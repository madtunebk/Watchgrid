//! Reading back the video track of a recording file: its H.264/HEVC parameters
//! and where each frame lies, from the `moov` box. Made for the files
//! `mp4.rs` writes (`ftyp`, `mdat`, `moov`), but follows the general box
//! rules (32/64-bit sizes, `stco` or `co64`, any `stsc` layout).

use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// One video frame in the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub offset: u64,
    pub size: u32,
    /// Start, in the track's timescale.
    pub time: u64,
    pub keyframe: bool,
}

#[derive(Debug, PartialEq)]
pub struct VideoIndex {
    pub codec: super::VideoCodec,
    /// AVC or HEVC decoder configuration record.
    pub decoder_config: Vec<u8>,
    pub timescale: u32,
    pub samples: Vec<Sample>,
}

impl VideoIndex {
    /// The keyframe at or before `secs` into the track (else the first one).
    pub fn keyframe_at(&self, secs: f64) -> Option<Sample> {
        let t = (secs.max(0.0) * f64::from(self.timescale)) as u64;
        let keys = self.samples.iter().filter(|s| s.keyframe);
        let first = keys.clone().next().copied();
        keys.take_while(|s| s.time <= t).last().copied().or(first)
    }
}

/// Children of a box payload: (type, payload).
fn children(mut buf: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    std::iter::from_fn(move || {
        if buf.len() < 8 {
            return None;
        }
        let size32 = u32::from_be_bytes(buf[0..4].try_into().ok()?);
        let kind: [u8; 4] = buf[4..8].try_into().ok()?;
        let (header, size) = match size32 {
            0 => (8, buf.len()),
            1 => (16, usize::try_from(u64::from_be_bytes(buf.get(8..16)?.try_into().ok()?)).ok()?),
            n => (8, n as usize),
        };
        if size < header || size > buf.len() {
            return None;
        }
        let payload = &buf[header..size];
        buf = &buf[size..];
        Some((kind, payload))
    })
}

fn child<'a>(buf: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    children(buf).find(|(k, _)| k == kind).map(|(_, p)| p)
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// The video track of a `moov` box's payload.
pub fn parse_moov(moov: &[u8]) -> Result<VideoIndex, String> {
    let mdia = children(moov)
        .filter(|(k, _)| k == b"trak")
        .filter_map(|(_, trak)| child(trak, b"mdia"))
        .find(|mdia| child(mdia, b"hdlr").and_then(|h| h.get(8..12)) == Some(b"vide"))
        .ok_or("no video track")?;
    let mdhd = child(mdia, b"mdhd").ok_or("no mdhd")?;
    let timescale = if mdhd.first() == Some(&1) { u32_at(mdhd, 20) } else { u32_at(mdhd, 12) }.ok_or("bad mdhd")?;
    let stbl = child(mdia, b"minf").and_then(|m| child(m, b"stbl")).ok_or("no sample table")?;
    let table = |k: &[u8; 4]| child(stbl, k);

    // Decoder config follows the 78-byte visual sample entry.
    let stsd = table(b"stsd").ok_or("no stsd")?;
    let entry = children(stsd.get(8..).ok_or("bad stsd")?).next().ok_or("empty stsd")?;
    let (codec, config) = match &entry.0 {
        b"avc1" => (super::VideoCodec::H264, b"avcC"),
        b"hvc1" => (super::VideoCodec::H265, b"hvcC"),
        _ => return Err("unsupported video sample entry".into()),
    };
    let decoder_config = child(entry.1.get(78..).ok_or("bad sample entry")?, config).ok_or("missing decoder configuration")?.to_vec();

    let stsz = table(b"stsz").ok_or("no stsz")?;
    let (fixed, count) = (u32_at(stsz, 4).ok_or("bad stsz")?, u32_at(stsz, 8).ok_or("bad stsz")? as usize);
    let sizes: Vec<u32> = if fixed != 0 { vec![fixed; count] } else { (0..count).map(|i| u32_at(stsz, 12 + i * 4)).collect::<Option<_>>().ok_or("bad stsz")? };

    let chunk_offsets: Vec<u64> = if let Some(co64) = table(b"co64") {
        let n = u32_at(co64, 4).ok_or("bad co64")? as usize;
        (0..n).map(|i| u64_at(co64, 8 + i * 8)).collect::<Option<_>>().ok_or("bad co64")?
    } else {
        let stco = table(b"stco").ok_or("no chunk offsets")?;
        let n = u32_at(stco, 4).ok_or("bad stco")? as usize;
        (0..n).map(|i| u32_at(stco, 8 + i * 4).map(u64::from)).collect::<Option<_>>().ok_or("bad stco")?
    };

    // stsc: runs of (first chunk, samples per chunk), 1-based.
    let stsc = table(b"stsc").ok_or("no stsc")?;
    let runs: Vec<(usize, usize)> = (0..u32_at(stsc, 4).ok_or("bad stsc")? as usize)
        .map(|i| Some((u32_at(stsc, 8 + i * 12)? as usize, u32_at(stsc, 12 + i * 12)? as usize)))
        .collect::<Option<_>>()
        .ok_or("bad stsc")?;
    let mut offsets = Vec::with_capacity(count);
    for (chunk, base) in chunk_offsets.iter().enumerate() {
        let per_chunk = runs.iter().take_while(|(first, _)| *first <= chunk + 1).last().map_or(1, |r| r.1);
        let mut at = *base;
        for _ in 0..per_chunk {
            let Some(size) = sizes.get(offsets.len()) else { break };
            offsets.push(at);
            at += u64::from(*size);
        }
    }
    if offsets.len() != count {
        return Err("sample tables disagree".into());
    }

    // stts: runs of (count, duration).
    let stts = table(b"stts").ok_or("no stts")?;
    let mut times = Vec::with_capacity(count);
    let mut t = 0u64;
    for i in 0..u32_at(stts, 4).ok_or("bad stts")? as usize {
        let (n, d) = (u32_at(stts, 8 + i * 8).ok_or("bad stts")?, u32_at(stts, 12 + i * 8).ok_or("bad stts")?);
        for _ in 0..n {
            times.push(t);
            t += u64::from(d);
        }
    }
    // No stss: every frame is a keyframe.
    let keys: Option<Vec<u32>> = table(b"stss")
        .map(|stss| (0..u32_at(stss, 4).unwrap_or(0) as usize).filter_map(|i| u32_at(stss, 8 + i * 4)).collect());
    let samples = (0..count)
        .map(|i| Sample {
            offset: offsets[i],
            size: sizes[i],
            time: times.get(i).copied().unwrap_or(t),
            keyframe: keys.as_ref().is_none_or(|k| k.binary_search(&(i as u32 + 1)).is_ok()),
        })
        .collect();
    Ok(VideoIndex { codec, decoder_config, timescale, samples })
}

/// Find and read the `moov` box of a finished recording.
pub async fn read_index(path: &std::path::Path) -> Result<VideoIndex, String> {
    let mut f = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    let len = f.metadata().await.map_err(|e| e.to_string())?.len();
    let mut at = 0u64;
    while at + 8 <= len {
        let mut header = [0u8; 16];
        f.seek(std::io::SeekFrom::Start(at)).await.map_err(|e| e.to_string())?;
        f.read_exact(&mut header[..8]).await.map_err(|e| e.to_string())?;
        let size32 = u32::from_be_bytes(header[0..4].try_into().expect("4 bytes"));
        let (header_len, size) = match size32 {
            0 => (8, len - at),
            1 => {
                f.read_exact(&mut header[8..16]).await.map_err(|e| e.to_string())?;
                (16, u64::from_be_bytes(header[8..16].try_into().expect("8 bytes")))
            }
            n => (8, u64::from(n)),
        };
        if size < header_len || at + size > len {
            break;
        }
        if &header[4..8] == b"moov" {
            let mut moov = vec![0u8; (size - header_len) as usize];
            f.seek(std::io::SeekFrom::Start(at + header_len)).await.map_err(|e| e.to_string())?;
            f.read_exact(&mut moov).await.map_err(|e| e.to_string())?;
            return parse_moov(&moov);
        }
        at += size;
    }
    Err("the file has no moov (not finished?)".into())
}

/// Read one frame's bytes.
pub async fn read_sample(path: &std::path::Path, s: Sample) -> Result<Vec<u8>, String> {
    let mut f = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    f.seek(std::io::SeekFrom::Start(s.offset)).await.map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; s.size as usize];
    f.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::boxes::VideoTrack;
    use crate::media::mp4::{SampleTable, file_start, mdat_size, moov};

    fn file(frames: &[(&[u8], u32, bool)]) -> Vec<u8> {
        let start = file_start();
        let mut file = start.bytes.clone();
        let mut table = SampleTable::default();
        for (data, duration, key) in frames {
            table.push(file.len() as u64, data.len() as u32, *duration, *key);
            file.extend_from_slice(data);
        }
        let at = start.mdat_size_at as usize;
        let size = mdat_size(file.len() as u64 - start.data_start);
        file[at..at + 8].copy_from_slice(&size.to_be_bytes());
        let track = VideoTrack { codec: crate::media::VideoCodec::H264, width: 640, height: 360, decoder_config: vec![1, 0x64, 0, 0x1e, 0xff, 0xe1, 0, 1, 0x67, 1, 0, 1, 0x68] };
        file.extend(moov(&track, &table, None));
        file
    }

    #[tokio::test]
    async fn reads_back_what_the_recorder_writes() {
        let frames: [(&[u8], u32, bool); 4] = [(b"key-1", 45_000, true), (b"p1", 45_000, false), (b"key-22", 45_000, true), (b"p2", 45_000, false)];
        let bytes = file(&frames);
        let path = std::env::temp_dir().join(format!("watchgrid-mp4read-{}.mp4", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();

        let index = read_index(&path).await.unwrap();
        assert_eq!(index.timescale, 90_000);
        assert_eq!(index.codec, super::super::VideoCodec::H264);
        assert_eq!(index.decoder_config[0..2], [1, 0x64]);
        assert_eq!(index.samples.iter().map(|s| (s.time, s.keyframe)).collect::<Vec<_>>(), [(0, true), (45_000, false), (90_000, true), (135_000, false)]);
        for (s, (data, _, _)) in index.samples.iter().zip(frames) {
            assert_eq!(read_sample(&path, *s).await.unwrap(), data);
        }
        // 1.2 s → the keyframe at 1.0 s; before the first → the first.
        assert_eq!(index.keyframe_at(1.2).map(|s| s.time), Some(90_000));
        assert_eq!(index.keyframe_at(0.9).map(|s| s.time), Some(0));
        std::fs::remove_file(path).unwrap();
    }
}
