//! ISO BMFF (MP4) box writing shared by the fragmented live stream and
//! recorded files: a byte writer plus the `ftyp`/`moov` structure for one
//! H.264 video track.

/// Track timescale: RTP video clock.
pub const TIMESCALE: u32 = 90_000;
/// Movie timescale (milliseconds).
const MOVIE_TIMESCALE: u32 = 1000;
const TRACK_ID: u32 = 1;

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

/// How the `moov` box is completed.
pub enum Layout<'a> {
    /// Fragmented: empty sample tables plus `mvex`; samples come in `moof`s.
    Fragmented,
    /// Complete file: `stbl` children written by the closure, total
    /// duration in [`TIMESCALE`] ticks.
    Progressive { duration: u64, sample_tables: &'a dyn Fn(&mut Writer) },
}

/// The `moov` box for one video track.
pub fn moov(w: &mut Writer, t: &VideoTrack, layout: Layout<'_>) {
    let duration = match layout {
        Layout::Fragmented => 0,
        Layout::Progressive { duration, .. } => duration,
    };
    // v0 boxes hold 32-bit durations; recordings are capped well below that.
    let media_duration = duration.min(u64::from(u32::MAX)) as u32;
    let movie_duration = (duration * u64::from(MOVIE_TIMESCALE) / u64::from(TIMESCALE)).min(u64::from(u32::MAX)) as u32;

    w.boxed(b"moov", |w| {
        w.full(b"mvhd", 0, 0, |w| {
            w.u32(0).u32(0).u32(MOVIE_TIMESCALE).u32(movie_duration).u32(0x0001_0000).u16(0x0100).zeros(10).matrix().zeros(24).u32(TRACK_ID + 1);
        });
        w.boxed(b"trak", |w| {
            w.full(b"tkhd", 0, 0x3, |w| {
                w.u32(0).u32(0).u32(TRACK_ID).u32(0).u32(movie_duration).zeros(8).u16(0).u16(0).u16(0).u16(0).matrix();
                w.u32(t.width << 16).u32(t.height << 16);
            });
            w.boxed(b"mdia", |w| {
                w.full(b"mdhd", 0, 0, |w| {
                    w.u32(0).u32(0).u32(TIMESCALE).u32(media_duration).u16(0x55c4).u16(0); // language "und"
                });
                w.full(b"hdlr", 0, 0, |w| {
                    w.u32(0).bytes(b"vide").zeros(12).bytes(b"Watchgrid\0");
                });
                w.boxed(b"minf", |w| {
                    w.full(b"vmhd", 0, 1, |w| {
                        w.zeros(8);
                    });
                    w.boxed(b"dinf", |w| {
                        w.full(b"dref", 0, 0, |w| {
                            w.u32(1).full(b"url ", 0, 1, |_| {});
                        });
                    });
                    w.boxed(b"stbl", |w| {
                        stsd(w, t);
                        match &layout {
                            Layout::Fragmented => empty_sample_tables(w),
                            Layout::Progressive { sample_tables, .. } => sample_tables(w),
                        }
                    });
                });
            });
        });
        if matches!(layout, Layout::Fragmented) {
            w.boxed(b"mvex", |w| {
                w.full(b"trex", 0, 0, |w| {
                    w.u32(TRACK_ID).u32(1).u32(0).u32(0).u32(0);
                });
            });
        }
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
