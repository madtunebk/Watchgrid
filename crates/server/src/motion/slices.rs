//! Just enough H.264 to tell a B slice: motion detection's decoder
//! (OpenH264) can't decode B-frames, and a camera that sends them (often a
//! "smart codec" / "H.264+" mode) should get a clear reason, not the
//! decoder's own error.

/// Does this frame (4-byte length-prefixed NAL units) contain a B slice?
pub fn has_b_slice(frame: &[u8]) -> bool {
    let mut i = 0;
    while let Some(len) = frame.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize) {
        let Some(nal) = frame.get(i + 4..i + 4 + len) else { return false };
        // 1: a slice of a non-IDR picture, 5: of an IDR picture.
        if matches!(nal.first().map(|h| h & 0x1f), Some(1 | 5)) && slice_type(&nal[1..]).is_some_and(|t| t % 5 == 1) {
            return true;
        }
        i += 4 + len;
    }
    false
}

/// `slice_type` from the start of a slice header: `first_mb_in_slice` then
/// `slice_type`, both Exp-Golomb coded.
fn slice_type(header: &[u8]) -> Option<u32> {
    let rbsp = unescape(&header[..header.len().min(16)]);
    let mut bits = Bits { data: &rbsp, pos: 0 };
    bits.ue()?; // first_mb_in_slice
    bits.ue()
}

/// Drop emulation-prevention bytes (`00 00 03` → `00 00`).
fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = self.data.get(self.pos / 8)?;
        let bit = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(u32::from(bit))
    }

    /// Unsigned Exp-Golomb.
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut value = 0u32;
        for _ in 0..zeros {
            value = (value << 1) | self.bit()?;
        }
        Some((1u32 << zeros) - 1 + value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One NAL unit, length-prefixed.
    fn frame(nal: &[u8]) -> Vec<u8> {
        let mut f = (nal.len() as u32).to_be_bytes().to_vec();
        f.extend_from_slice(nal);
        f
    }

    #[test]
    fn tells_b_slices_from_p_and_i() {
        // first_mb_in_slice = 0 ("1"), then slice_type:
        assert!(!has_b_slice(&frame(&[0x41, 0b1100_0000])), "P (0: \"1\")");
        assert!(has_b_slice(&frame(&[0x01, 0b1010_0000])), "B (1: \"010\")");
        assert!(!has_b_slice(&frame(&[0x65, 0b1011_0000])), "I (2: \"011\")");
        assert!(has_b_slice(&frame(&[0x01, 0b1001_1100])), "B, all slices alike (6: \"00111\")");
        assert!(!has_b_slice(&frame(&[0x41, 0b1001_1000])), "P, all slices alike (5: \"00110\")");
    }

    #[test]
    fn skips_other_units_and_survives_garbage() {
        let mut f = frame(&[0x67, 0x64, 0x00, 0x1f]); // SPS: not a slice
        f.extend(frame(&[0x01, 0b1010_0000]));
        assert!(has_b_slice(&f), "the B slice after the SPS counts");
        assert!(!has_b_slice(&[0, 0, 0, 9, 1]), "cut short");
        assert!(!has_b_slice(&frame(&[0x01, 0, 0, 0])), "no valid header");
    }

    #[test]
    fn emulation_prevention_is_removed() {
        assert_eq!(unescape(&[0, 0, 3, 1, 0, 0, 3]), [0, 0, 1, 0, 0]);
    }
}
