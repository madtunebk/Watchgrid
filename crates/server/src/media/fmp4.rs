//! Fragmented MP4 for Media Source Extensions: an init segment
//! (`ftyp` + `moov`) followed by media segments (`moof` + `mdat`), one
//! sample each for low latency. Video samples are the camera's own H.264
//! access units — nothing is decoded; audio samples are Opus packets.

use super::audio::AudioTrack;
use super::boxes::{self, Media, Trak, VideoTrack, Writer};

/// `ftyp` + `moov` for the video track and, if any, the audio track.
pub fn init_segment(video: &VideoTrack, audio: Option<&AudioTrack>) -> Vec<u8> {
    let mut w = Writer::new();
    boxes::ftyp(&mut w);
    let mut traks = vec![Trak { media: Media::Video(video), duration: 0, sample_tables: None }];
    if let Some(a) = audio {
        traks.push(Trak { media: Media::Audio(a), duration: 0, sample_tables: None });
    }
    boxes::moov(&mut w, &traks);
    w.0
}

/// One sample of track `track` as `moof` + `mdat`. `decode_time` and
/// `duration` are in the track's timescale; audio samples are always sync.
pub fn media_segment(sequence: u32, track: u32, decode_time: u64, duration: u32, keyframe: bool, data: &[u8]) -> Vec<u8> {
    // trun flags: data-offset, sample-duration, sample-size, sample-flags.
    const TRUN_FLAGS: u32 = 0x000001 | 0x000100 | 0x000200 | 0x000400;
    // Sync sample vs. non-sync sample that depends on others.
    let sample_flags = if keyframe { 0x0200_0000 } else { 0x0101_0000 };

    let mut w = Writer::new();
    let mut data_offset_at = 0;
    w.boxed(b"moof", |w| {
        w.full(b"mfhd", 0, 0, |w| {
            w.u32(sequence);
        });
        w.boxed(b"traf", |w| {
            // default-base-is-moof: data offsets are relative to this moof.
            w.full(b"tfhd", 0, 0x020000, |w| {
                w.u32(track);
            });
            w.full(b"tfdt", 1, 0, |w| {
                w.u64(decode_time);
            });
            w.full(b"trun", 0, TRUN_FLAGS, |w| {
                w.u32(1);
                data_offset_at = w.len();
                w.u32(0).u32(duration).u32(data.len() as u32).u32(sample_flags);
            });
        });
    });
    // Data starts right after moof and the 8-byte mdat header.
    let data_offset = (w.len() + 8) as u32;
    w.patch_u32(data_offset_at, data_offset);
    w.boxed(b"mdat", |w| {
        w.bytes(data);
    });
    w.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::boxes::TIMESCALE;
    use crate::media::boxes::test_util::{find, top_level, u32_at};

    #[test]
    fn init_segment_is_ftyp_then_moov_with_avcc() {
        let track = VideoTrack { width: 1280, height: 720, avcc: vec![1, 0x64, 0, 0x1f, 0xff] };
        let init = init_segment(&track, None);
        let boxes: Vec<String> = top_level(&init).into_iter().map(|b| b.0).collect();
        assert_eq!(boxes, ["ftyp", "moov"]);
        let avcc = find(&init, b"avcC");
        assert_eq!(&init[avcc + 8..avcc + 13], &[1, 0x64, 0, 0x1f, 0xff]);
        let mdhd = find(&init, b"mdhd");
        assert_eq!(u32_at(&init, mdhd + 20), TIMESCALE);
        find(&init, b"mvex");
    }

    #[test]
    fn media_segment_offsets_point_at_the_sample() {
        let data = [0u8, 0, 0, 3, 0x65, 0xaa, 0xbb];
        let seg = media_segment(7, 1, 90_000, 3600, true, &data);
        let boxes = top_level(&seg);
        assert_eq!(boxes.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["moof", "mdat"]);

        let trun = find(&seg, b"trun");
        let offset = u32_at(&seg, trun + 16) as usize;
        assert_eq!(&seg[offset..offset + data.len()], &data, "data offset is relative to moof start");
        assert_eq!(u32_at(&seg, trun + 20), 3600);
        assert_eq!(u32_at(&seg, trun + 28), 0x0200_0000, "keyframe is a sync sample");

        let tfdt = find(&seg, b"tfdt");
        assert_eq!(u64::from_be_bytes(seg[tfdt + 12..tfdt + 20].try_into().unwrap()), 90_000);
        let mfhd = find(&seg, b"mfhd");
        assert_eq!(u32_at(&seg, mfhd + 12), 7);
    }

    #[test]
    fn non_keyframes_are_marked_dependent() {
        let seg = media_segment(1, 1, 0, 3000, false, &[0, 0, 0, 1, 0x41]);
        let trun = find(&seg, b"trun");
        assert_eq!(u32_at(&seg, trun + 28), 0x0101_0000);
    }

    #[test]
    fn init_segment_with_audio_has_an_opus_track() {
        let video = VideoTrack { width: 640, height: 360, avcc: vec![1, 0x64, 0, 0x1e, 0xff] };
        let audio = AudioTrack { channels: 1, pre_skip: 312, input_rate: 8000 };
        let init = init_segment(&video, Some(&audio));
        assert_eq!(init.windows(4).filter(|w| *w == b"trak").count(), 2);
        assert_eq!(init.windows(4).filter(|w| *w == b"trex").count(), 2);
        let dops = find(&init, b"dOps");
        assert_eq!(&init[dops + 8..dops + 10], &[0, 1], "version 0, mono");
        assert_eq!(u16::from_be_bytes([init[dops + 10], init[dops + 11]]), 312);
        assert_eq!(u32_at(&init, dops + 12), 48_000, "Chrome wants the sample entry's rate here");
        let seg = media_segment(3, 2, 960, 960, true, &[0xfc, 1]);
        let tfhd = find(&seg, b"tfhd");
        assert_eq!(u32_at(&seg, tfhd + 12), 2, "audio track id");
    }
}
