use super::boxes::test_util::{find, top_level, u32_at};
use super::{Frame, TrackInfo, VideoCodec, VideoTrack, fmp4, mp4_read};

#[tokio::test]
async fn hevc_survives_live_muxing_and_recording() {
    let source = include_bytes!("testdata/hevc.mp4");
    let moov = find(source, b"moov");
    let input = mp4_read::parse_moov(&source[moov + 8..moov + u32_at(source, moov) as usize]).unwrap();
    assert_eq!(input.codec, VideoCodec::H265);
    let track = VideoTrack { codec: input.codec, width: 320, height: 180, decoder_config: input.decoder_config.clone() };
    let info = TrackInfo { codec: "hvc1.1.6.L60.90".into(), track: track.clone(), audio_codec: None, audio: None };
    assert!(info.can_mux());
    assert!(!info.is_h264(), "HEVC must not enter the software H.264 decoder");
    assert_eq!(info.codec_label(), "HEVC");

    let mut live = fmp4::init_segment(&track, None);
    assert_eq!(top_level(&live).iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["ftyp", "moov"]);
    find(&live, b"hvc1");
    let hvcc = find(&live, b"hvcC");
    assert_eq!(&live[hvcc + 8..hvcc + u32_at(&live, hvcc) as usize], input.decoder_config);
    assert!(!live.windows(4).any(|w| w == b"avcC" || w == b"avc1"));

    let path = std::env::temp_dir().join(format!("watchgrid-hevc-{}.mp4", std::process::id()));
    let mut writer = crate::recorder::writer::Mp4Writer::create(path.clone(), None).await.unwrap();
    for (i, sample) in input.samples.iter().enumerate() {
        let data = &source[sample.offset as usize..sample.offset as usize + sample.size as usize];
        let pts = (sample.time * u64::from(super::TIMESCALE) / u64::from(input.timescale)) as i64;
        live.extend(fmp4::media_segment(i as u32 + 1, 1, pts as u64, 18_000, sample.keyframe, data));
        writer.push(Frame { pts, keyframe: sample.keyframe, data: data.to_vec().into() }).await.unwrap();
    }
    writer.finish(&track).await.unwrap();
    let recorded = mp4_read::read_index(&path).await.unwrap();
    assert_eq!(recorded.codec, VideoCodec::H265);
    assert_eq!(recorded.decoder_config, input.decoder_config);
    assert_eq!(recorded.samples.len(), input.samples.len());
    for (before, after) in input.samples.iter().zip(&recorded.samples) {
        assert_eq!(after.keyframe, before.keyframe);
        assert_eq!(after.time * u64::from(input.timescale), before.time * u64::from(recorded.timescale));
        assert_eq!(mp4_read::read_sample(&path, *after).await.unwrap(), source[before.offset as usize..before.offset as usize + before.size as usize]);
    }
    // Optional artifacts for independent ffprobe/decoder verification.
    if let Ok(dir) = std::env::var("WATCHGRID_HEVC_TEST_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("hevc-live.mp4"), live).unwrap();
        std::fs::copy(&path, std::path::Path::new(&dir).join("hevc-recorded.mp4")).unwrap();
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn only_known_mp4_codecs_are_admitted() {
    assert_eq!(VideoCodec::from_rfc6381("hvc1.1.6.L90.B0"), VideoCodec::H265);
    assert_eq!(VideoCodec::from_rfc6381("avc1.640028"), VideoCodec::H264);
    for codec in ["mp4v.20.9", "hvec1", "avc123", ""] {
        assert_eq!(VideoCodec::from_rfc6381(codec), VideoCodec::Unsupported);
    }
}
