//! H.264 access-unit clean-up. Frames are AVC "length-prefixed" NAL units
//! (4-byte big-endian length, then the NAL).
//!
//! SEI units (type 6) carry optional metadata only. Some cameras (e.g.
//! Tapo TC72 main stream) send malformed vendor SEI in every frame, which
//! can make browser decoders stall after the first picture; they are
//! dropped before anything else sees the frame.

const SEI: u8 = 6;

/// The frame without SEI units; unchanged (no copy) if it has none or
/// isn't well-formed length-prefixed data.
pub fn strip_sei(data: Vec<u8>) -> Vec<u8> {
    let Some(units) = split(&data) else { return data };
    if !units.iter().any(|(start, end)| nal_type(&data[*start..*end]) == Some(SEI)) {
        return data;
    }
    let mut out = Vec::with_capacity(data.len());
    for (start, end) in units {
        if nal_type(&data[start..end]) != Some(SEI) {
            out.extend_from_slice(&((end - start) as u32).to_be_bytes());
            out.extend_from_slice(&data[start..end]);
        }
    }
    out
}

fn nal_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|b| b & 0x1f)
}

/// `(start, end)` of each NAL payload, or `None` if lengths don't add up.
fn split(data: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut units = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let len = u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?) as usize;
        let (start, end) = (i + 4, i + 4 + len);
        if end > data.len() || len == 0 {
            return None;
        }
        units.push((start, end));
        i = end;
    }
    Some(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nal(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 1) as u32).to_be_bytes().to_vec();
        v.push(kind);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn drops_sei_and_keeps_everything_else_in_order() {
        let frame = [nal(0x06, &[0x05, 0x32, 1, 2]), nal(0x65, &[9, 9, 9]), nal(0x06, &[1]), nal(0x41, &[7])].concat();
        assert_eq!(strip_sei(frame), [nal(0x65, &[9, 9, 9]), nal(0x41, &[7])].concat());
    }

    #[test]
    fn leaves_clean_or_odd_frames_alone() {
        let clean = [nal(0x67, &[1]), nal(0x65, &[2])].concat();
        assert_eq!(strip_sei(clean.clone()), clean);
        let broken = vec![0, 0, 0, 9, 0x06, 1];
        assert_eq!(strip_sei(broken.clone()), broken, "lengths don't add up: untouched");
    }
}
