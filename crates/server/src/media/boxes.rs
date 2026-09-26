//! ISO BMFF (MP4) box writing shared by the fragmented live stream and
//! recorded files: a byte writer plus the `ftyp`/`moov` structure for an
//! H.264 video track and, when the camera has audio, an Opus track.

use super::audio::{AudioTrack, OPUS_TIMESCALE};

/// Track timescale: RTP video clock.
pub const TIMESCALE: u32 = 90_000;
/// Movie timescale (milliseconds).
const MOVIE_TIMESCALE: u32 = 1000;

/// Everything needed to describe the track.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoTrack {
    pub width: u32,
    pub height: u32,
    /// AVCDecoderConfigurationRecord (the `avcC` box payload).
    pub avcc: Vec<u8>,
}

pub struct Writer(pub Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self(Vec::with_capacity(1024))
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn zeros(&mut self, n: usize) -> &mut Self {
        self.0.resize(self.0.len() + n, 0);
        self
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Overwrite a u32 written earlier (e.g. an offset known only later).
    pub fn patch_u32(&mut self, at: usize, v: u32) {
        self.0[at..at + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Write a box: size, type, then whatever `body` writes.
    pub fn boxed(&mut self, kind: &[u8; 4], body: impl FnOnce(&mut Self)) -> &mut Self {
        let start = self.0.len();
        self.u32(0).bytes(kind);
        body(self);
        let size = (self.0.len() - start) as u32;
        self.patch_u32(start, size);
        self
    }

    /// Full box: version + 24-bit flags.
    pub fn full(&mut self, kind: &[u8; 4], version: u8, flags: u32, body: impl FnOnce(&mut Self)) -> &mut Self {
        self.boxed(kind, |w| {
            w.u32((u32::from(version) << 24) | (flags & 0x00ff_ffff));
            body(w);
        })
    }

    fn matrix(&mut self) -> &mut Self {
        for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            self.u32(v);
        }
        self
    }
}

pub fn ftyp(w: &mut Writer) {
    w.boxed(b"ftyp", |w| {
        w.bytes(b"isom").u32(0x200).bytes(b"isom").bytes(b"iso6").bytes(b"avc1").bytes(b"mp41");
    });
}

/// Track ids: video first, then audio when the camera has it.
pub const VIDEO_TRACK: u32 = 1;
pub const AUDIO_TRACK: u32 = 2;

/// What a track carries.
pub enum Media<'a> {
    Video(&'a VideoTrack),
    Audio(&'a AudioTrack),
}

impl Media<'_> {
    fn id(&self) -> u32 {
        match self {
            Self::Video(_) => VIDEO_TRACK,
            Self::Audio(_) => AUDIO_TRACK,
        }
    }

    pub fn timescale(&self) -> u32 {
        match self {
            Self::Video(_) => TIMESCALE,
            Self::Audio(_) => OPUS_TIMESCALE,
        }
    }
}

/// One `trak`: its media, duration (in the media's timescale) and, for a
/// complete file, a closure writing the sample tables (`None`: fragmented).
pub struct Trak<'a> {
    pub media: Media<'a>,
    pub duration: u64,
    pub sample_tables: Option<&'a dyn Fn(&mut Writer)>,
}

/// The `moov` box. Fragmented when no track has sample tables: empty
/// tables plus `mvex`, samples then come in `moof`s.
pub fn moov(w: &mut Writer, traks: &[Trak<'_>]) {
    let fragmented = traks.iter().all(|t| t.sample_tables.is_none());
    let movie_ms = |t: &Trak<'_>| t.duration * u64::from(MOVIE_TIMESCALE) / u64::from(t.media.timescale());
    // v0 boxes hold 32-bit durations; recordings are capped well below that.
    let clamp = |v: u64| v.min(u64::from(u32::MAX)) as u32;
    let movie_duration = clamp(traks.iter().map(movie_ms).max().unwrap_or(0));

    w.boxed(b"moov", |w| {
        w.full(b"mvhd", 0, 0, |w| {
            w.u32(0).u32(0).u32(MOVIE_TIMESCALE).u32(movie_duration).u32(0x0001_0000).u16(0x0100).zeros(10).matrix().zeros(24).u32(AUDIO_TRACK + 1);
        });
        for t in traks {
            trak(w, t, clamp(movie_ms(t)), clamp(t.duration));
        }
        if fragmented {
            w.boxed(b"mvex", |w| {
                for t in traks {
                    w.full(b"trex", 0, 0, |w| {
                        w.u32(t.media.id()).u32(1).u32(0).u32(0).u32(0);
                    });
                }
            });
        }
    });
}

fn trak(w: &mut Writer, t: &Trak<'_>, movie_duration: u32, media_duration: u32) {
    let (width, height, volume) = match t.media {
        Media::Video(v) => (v.width, v.height, 0),
        Media::Audio(_) => (0, 0, 0x0100),
    };
    w.boxed(b"trak", |w| {
        w.full(b"tkhd", 0, 0x3, |w| {
            w.u32(0).u32(0).u32(t.media.id()).u32(0).u32(movie_duration).zeros(8).u16(0).u16(0).u16(volume).u16(0).matrix();
            w.u32(width << 16).u32(height << 16);
        });
        w.boxed(b"mdia", |w| {
            w.full(b"mdhd", 0, 0, |w| {
                w.u32(0).u32(0).u32(t.media.timescale()).u32(media_duration).u16(0x55c4).u16(0); // language "und"
            });
            let handler = if matches!(t.media, Media::Video(_)) { b"vide" } else { b"soun" };
            w.full(b"hdlr", 0, 0, |w| {
                w.u32(0).bytes(handler).zeros(12).bytes(b"Watchgrid\0");
            });
            w.boxed(b"minf", |w| {
                match t.media {
                    Media::Video(_) => w.full(b"vmhd", 0, 1, |w| {
                        w.zeros(8);
                    }),
                    Media::Audio(_) => w.full(b"smhd", 0, 0, |w| {
                        w.zeros(4);
                    }),
                };
                w.boxed(b"dinf", |w| {
                    w.full(b"dref", 0, 0, |w| {
                        w.u32(1).full(b"url ", 0, 1, |_| {});
                    });
                });
                w.boxed(b"stbl", |w| {
                    match t.media {
                        Media::Video(v) => stsd(w, v),
                        Media::Audio(a) => stsd_opus(w, a),
                    }
                    match t.sample_tables {
                        Some(tables) => tables(w),
                        None => empty_sample_tables(w),
                    }
                });
            });
        });
    });
}

/// `Opus` sample entry with its `dOps` box (Opus in ISO BMFF, 4.3).
fn stsd_opus(w: &mut Writer, a: &AudioTrack) {
    w.full(b"stsd", 0, 0, |w| {
        w.u32(1).boxed(b"Opus", |w| {
            w.zeros(6).u16(1); // reserved, data_reference_index
            w.zeros(8).u16(u16::from(a.channels)).u16(16).u16(0).u16(0).u32(OPUS_TIMESCALE << 16);
            // InputSampleRate is informational, but Chrome rejects a value
            // other than the sample entry's rate (48 kHz), so not 8000.
            w.boxed(b"dOps", |w| {
                w.bytes(&[0, a.channels]).u16(a.pre_skip).u32(OPUS_TIMESCALE).u16(0).bytes(&[0]);
            });
        });
    });
}

fn stsd(w: &mut Writer, t: &VideoTrack) {
    w.full(b"stsd", 0, 0, |w| {
        w.u32(1).boxed(b"avc1", |w| {
            w.zeros(6).u16(1); // reserved, data_reference_index
            w.zeros(16).u16(t.width as u16).u16(t.height as u16);
            w.u32(0x0048_0000).u32(0x0048_0000).u32(0).u16(1);
            w.zeros(32).u16(0x0018).u16(0xffff);
            w.boxed(b"avcC", |w| {
                w.bytes(&t.avcc);
            });
        });
    });
}

fn empty_sample_tables(w: &mut Writer) {
    w.full(b"stts", 0, 0, |w| {
        w.u32(0);
    });
    w.full(b"stsc", 0, 0, |w| {
        w.u32(0);
    });
    w.full(b"stsz", 0, 0, |w| {
        w.u32(0).u32(0);
    });
    w.full(b"stco", 0, 0, |w| {
        w.u32(0);
    });
}

#[cfg(test)]
pub mod test_util {
    /// Top-level boxes as (type, size); asserts sizes tile the buffer exactly.
    pub fn top_level(buf: &[u8]) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < buf.len() {
            let mut size = u32::from_be_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
            if size == 1 {
                size = u64::from_be_bytes(buf[i + 8..i + 16].try_into().unwrap()) as usize;
            }
            out.push((String::from_utf8_lossy(&buf[i + 4..i + 8]).into_owned(), size));
            assert!(size >= 8 && i + size <= buf.len(), "box overruns buffer");
            i += size;
        }
        assert_eq!(i, buf.len());
        out
    }

    /// Offset of the first box of this type (searching anywhere).
    pub fn find(buf: &[u8], kind: &[u8; 4]) -> usize {
        buf.windows(4).position(|w| w == kind).expect("box present") - 4
    }

    pub fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_be_bytes(buf[at..at + 4].try_into().unwrap())
    }
}
