//! One H.264 keyframe → a small JPEG. Scaled straight from the decoder's
//! YUV planes (a box average of luma per output pixel), so a 1440p frame
//! never becomes a full-size RGB copy.

use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

use crate::motion::decoder::{annex_b, parameter_sets};

const QUALITY: u8 = 75;

/// Decode `frame` (a keyframe, length-prefixed NAL units) with the
/// stream's `avcc` parameters and return a JPEG at most `max_width` wide.
pub fn jpeg(avcc: &[u8], frame: &[u8], max_width: usize) -> Result<Vec<u8>, String> {
    let params = parameter_sets(avcc).ok_or("the stream has no H.264 parameters")?;
    let data = annex_b(frame).ok_or("malformed frame")?;
    let mut decoder = Decoder::new().map_err(|e| format!("cannot create the H.264 decoder: {e}"))?;
    decoder.decode(&params).map_err(|e| format!("bad H.264 parameters: {e}"))?;
    let rgb_and_size = match decoder.decode(&data).map_err(|e| e.to_string())? {
        Some(p) => scale(&p, max_width),
        None => {
            let rest = decoder.flush_remaining().map_err(|e| e.to_string())?;
            scale(rest.first().ok_or("the frame produced no picture")?, max_width)
        }
    };
    let (rgb, w, h) = rgb_and_size;
    encode(&rgb, w, h)
}

fn encode(rgb: &[u8], w: usize, h: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(16 * 1024);
    let encoder = jpeg_encoder::Encoder::new(&mut out, QUALITY);
    encoder.encode(rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb).map_err(|e| e.to_string())?;
    Ok(out)
}

/// BT.601 limited range → RGB.
fn rgb(y: f32, u: f32, v: f32) -> [u8; 3] {
    let (c, d, e) = (1.164 * (y - 16.0), u - 128.0, v - 128.0);
    let px = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    [px(c + 1.596 * e), px(c - 0.392 * d - 0.813 * e), px(c + 2.017 * d)]
}

/// Downscale to at most `max_width` (keeping the aspect ratio) as RGB.
fn scale(p: &impl YUVSource, max_width: usize) -> (Vec<u8>, usize, usize) {
    let (sw, sh) = p.dimensions();
    let (ys, us, vs) = p.strides();
    let (y, u, v) = (p.y(), p.u(), p.v());
    let w = max_width.min(sw).max(1);
    let h = (sh * w / sw).max(1);
    let mut out = Vec::with_capacity(w * h * 3);
    for oy in 0..h {
        let (y0, y1) = (oy * sh / h, ((oy + 1) * sh / h).max(oy * sh / h + 1));
        for ox in 0..w {
            let (x0, x1) = (ox * sw / w, ((ox + 1) * sw / w).max(ox * sw / w + 1));
            let mut sum = 0u32;
            for sy in y0..y1 {
                sum += y[sy * ys + x0..sy * ys + x1].iter().map(|&b| u32::from(b)).sum::<u32>();
            }
            let luma = sum as f32 / ((y1 - y0) * (x1 - x0)) as f32;
            let (cx, cy) = ((x0 + x1) / 4, (y0 + y1) / 4);
            out.extend(rgb(luma, f32::from(u[cy * us + cx]), f32::from(v[cy * vs + cx])));
        }
    }
    (out, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_convert_like_bt601() {
        assert_eq!(rgb(16.0, 128.0, 128.0), [0, 0, 0]);
        assert_eq!(rgb(235.0, 128.0, 128.0), [255, 255, 255]);
        let red = rgb(81.0, 90.0, 240.0);
        assert!(red[0] > 240 && red[1] < 10 && red[2] < 10, "{red:?}");
    }

    #[test]
    fn a_small_picture_encodes_as_jpeg() {
        let rgb = vec![128u8; 32 * 18 * 3];
        let jpeg = encode(&rgb, 32, 18).unwrap();
        assert_eq!(&jpeg[..2], [0xff, 0xd8], "JPEG start marker");
    }
}
