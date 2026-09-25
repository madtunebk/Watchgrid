//! H.264 decoding for motion analysis: Watchgrid frames (length-prefixed
//! NAL units, parameters in the avcC record) become Annex B for OpenH264,
//! and only the luma plane of each picture is used.

use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// SPS and PPS from an AVCDecoderConfigurationRecord, as Annex B.
pub fn parameter_sets(avcc: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 5;
    let sps_count = usize::from(*avcc.get(i)? & 0x1f);
    i += 1;
    for _ in 0..sps_count {
        i = copy_unit(avcc, i, &mut out)?;
    }
    let pps_count = usize::from(*avcc.get(i)?);
    i += 1;
    for _ in 0..pps_count {
        i = copy_unit(avcc, i, &mut out)?;
    }
    (!out.is_empty()).then_some(out)
}

/// Copy one 16-bit-length-prefixed unit at `i`; returns the next offset.
fn copy_unit(avcc: &[u8], i: usize, out: &mut Vec<u8>) -> Option<usize> {
    let len = usize::from(u16::from_be_bytes([*avcc.get(i)?, *avcc.get(i + 1)?]));
    let unit = avcc.get(i + 2..i + 2 + len)?;
    out.extend_from_slice(&START_CODE);
    out.extend_from_slice(unit);
    Some(i + 2 + len)
}

/// A frame's 4-byte-length-prefixed NAL units as Annex B.
pub fn annex_b(frame: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(frame.len() + 16);
    let mut i = 0;
    while i < frame.len() {
        let len = u32::from_be_bytes(frame.get(i..i + 4)?.try_into().ok()?) as usize;
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(frame.get(i + 4..i + 4 + len)?);
        i += 4 + len;
    }
    Some(out)
}

/// A decoded picture's luma plane.
pub struct Luma<'a> {
    pub y: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}

pub struct H264 {
    decoder: Decoder,
}

impl H264 {
    /// A decoder primed with the stream's parameter sets.
    pub fn new(avcc: &[u8]) -> Result<Self, String> {
        let params = parameter_sets(avcc).ok_or("the stream has no H.264 parameters")?;
        let mut decoder = Decoder::new().map_err(|e| format!("cannot create the H.264 decoder: {e}"))?;
        decoder.decode(&params).map_err(|e| format!("bad H.264 parameters: {e}"))?;
        Ok(Self { decoder })
    }

    /// Decode one frame and hand its picture to `use_picture`; `None` when
    /// the frame produced no picture.
    pub fn decode<R>(&mut self, frame: &[u8], use_picture: impl FnOnce(Luma<'_>) -> R) -> Result<Option<R>, String> {
        let data = annex_b(frame).ok_or("malformed frame")?;
        let picture = self.decoder.decode(&data).map_err(|e| e.to_string())?;
        Ok(picture.map(|p| {
            let (width, height) = p.dimensions();
            let stride = p.strides().0;
            use_picture(Luma { y: p.y(), width, height, stride })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_avcc_parameters_and_frames_to_annex_b() {
        // version, profile, compat, level, 0xff (4-byte lengths), 1 SPS, 1 PPS
        let avcc = [1, 0x42, 0, 0x1e, 0xff, 0xe1, 0, 2, 0x67, 0xaa, 1, 0, 1, 0x68];
        assert_eq!(parameter_sets(&avcc).unwrap(), [0, 0, 0, 1, 0x67, 0xaa, 0, 0, 0, 1, 0x68]);
        assert_eq!(parameter_sets(&avcc[..9]), None, "truncated");

        let frame = [0, 0, 0, 2, 0x65, 1, 0, 0, 0, 1, 0x41];
        assert_eq!(annex_b(&frame).unwrap(), [0, 0, 0, 1, 0x65, 1, 0, 0, 0, 1, 0x41]);
        assert_eq!(annex_b(&[0, 0, 0, 9, 0x65]), None, "length past the end");
    }

    #[test]
    fn rejects_streams_without_parameters() {
        assert!(H264::new(&[1, 0x42, 0, 0x1e, 0xff, 0xe0, 0]).is_err());
    }
}
