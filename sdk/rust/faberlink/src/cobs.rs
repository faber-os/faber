//! Consistent Overhead Byte Stuffing: removes every `0x00` from a block so that `0x00` can mark
//! the end of a frame. A receiver that joins mid-stream, or that sees garbage, finds the next
//! frame at the next `0x00`.
//!
//! The encoding is a list of groups. Each group starts with a code byte `n` (1..=255) followed by
//! `n - 1` data bytes; a group with `n < 255` stands for its data bytes followed by a `0x00`
//! (except the very last group, whose implied zero is dropped). FaberLink frames are at most 254
//! bytes, so their encoding is always exactly one byte longer than the frame.

/// The encoded length of `len` raw bytes, worst case.
pub const fn max_encoded_len(len: usize) -> usize {
    len + len / 254 + 1
}

/// Encodes `raw` into `out` (no delimiter) and returns the encoded length, or `None` if `out` is
/// too small (it needs [`max_encoded_len`] bytes).
pub fn encode(raw: &[u8], out: &mut [u8]) -> Option<usize> {
    if out.len() < max_encoded_len(raw.len()) {
        return None;
    }
    let mut code_at = 0;
    let mut code: u8 = 1;
    let mut w = 1;
    for (i, &b) in raw.iter().enumerate() {
        if b == 0 {
            out[code_at] = code;
            code_at = w;
            w += 1;
            code = 1;
        } else {
            out[w] = b;
            w += 1;
            code += 1;
            if code == 0xFF {
                out[code_at] = code;
                if i + 1 == raw.len() {
                    // A full group at the very end needs no empty group after it.
                    return Some(w);
                }
                code_at = w;
                w += 1;
                code = 1;
            }
        }
    }
    out[code_at] = code;
    Some(w)
}

/// Why a block is not valid COBS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invalid;

/// Decodes `buf[..len]` in place and returns the decoded length. Decoding never makes a block
/// longer, so the output fits where the input was.
pub fn decode_in_place(buf: &mut [u8], len: usize) -> Result<usize, Invalid> {
    let mut r = 0;
    let mut w = 0;
    while r < len {
        let code = buf[r] as usize;
        if code == 0 {
            return Err(Invalid);
        }
        r += 1;
        let end = r + code - 1;
        if end > len {
            return Err(Invalid);
        }
        while r < end {
            if buf[r] == 0 {
                return Err(Invalid);
            }
            buf[w] = buf[r];
            w += 1;
            r += 1;
        }
        if code != 0xFF && r < len {
            buf[w] = 0;
            w += 1;
        }
    }
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn enc(raw: &[u8]) -> Vec<u8> {
        let mut out = vec![0; max_encoded_len(raw.len())];
        let n = encode(raw, &mut out).unwrap();
        out.truncate(n);
        out
    }

    fn dec(encoded: &[u8]) -> Result<Vec<u8>, Invalid> {
        let mut buf = encoded.to_vec();
        let n = decode_in_place(&mut buf, encoded.len())?;
        buf.truncate(n);
        Ok(buf)
    }

    #[test]
    fn known_vectors() {
        // From the COBS paper / Wikipedia.
        assert_eq!(enc(&[]), [0x01]);
        assert_eq!(enc(&[0x00]), [0x01, 0x01]);
        assert_eq!(enc(&[0x00, 0x00]), [0x01, 0x01, 0x01]);
        assert_eq!(enc(&[0x11, 0x22, 0x00, 0x33]), [0x03, 0x11, 0x22, 0x02, 0x33]);
        assert_eq!(enc(&[0x11, 0x00, 0x00, 0x00]), [0x02, 0x11, 0x01, 0x01, 0x01]);
        let run: Vec<u8> = (1..=254).collect();
        assert_eq!(enc(&run).len(), 255, "a 254-byte frame grows by exactly one byte");
        let mut expected = vec![0xFF];
        expected.extend(&run);
        assert_eq!(enc(&run), expected);
    }

    #[test]
    fn rejects_zeros_and_overruns() {
        assert_eq!(dec(&[0x00]), Err(Invalid));
        assert_eq!(dec(&[0x05, 0x11]), Err(Invalid));
        assert_eq!(dec(&[0x03, 0x11, 0x00]), Err(Invalid));
    }

    #[test]
    fn a_too_small_buffer_is_refused() {
        assert_eq!(encode(&[1, 2, 3], &mut [0; 3]), None);
    }

    proptest! {
        #[test]
        fn round_trips_without_zeros(raw in proptest::collection::vec(any::<u8>(), 0..600)) {
            let e = enc(&raw);
            prop_assert!(!e.contains(&0));
            prop_assert!(e.len() <= max_encoded_len(raw.len()));
            prop_assert_eq!(dec(&e).unwrap(), raw);
        }

        #[test]
        fn decoding_garbage_never_panics(junk in proptest::collection::vec(any::<u8>(), 0..300)) {
            let _ = dec(&junk);
        }
    }
}
