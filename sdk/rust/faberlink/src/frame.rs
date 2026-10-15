//! Frames: `type (u8) · seq (u8) · payload (0..=250 B) · CRC-16 (u16 LE)`, COBS-encoded and
//! followed by one `0x00`. The CRC covers type, seq and payload.
//!
//! This module needs neither `std` nor an allocator.

use crate::{cobs, crc::crc16};

/// The largest payload a frame may carry.
pub const MAX_PAYLOAD: usize = 250;
/// The largest frame before encoding: type, seq, payload and CRC.
pub const MAX_RAW: usize = MAX_PAYLOAD + 4;
/// The largest frame on the wire, including the `0x00` delimiter.
pub const MAX_WIRE: usize = MAX_RAW + 2;

/// One checked frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame<'a> {
    pub kind: u8,
    pub seq: u8,
    pub payload: &'a [u8],
}

/// Why bytes between two `0x00`s were dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadFrame {
    /// Not valid COBS.
    Cobs,
    /// Shorter than type, seq and CRC.
    TooShort,
    /// The CRC doesn't match: a bit was flipped on the wire.
    Crc,
    /// More bytes than the largest frame before the next `0x00`.
    TooLong,
}

/// Encodes one frame for the wire into `out` (which needs [`MAX_WIRE`] bytes for the largest
/// frame) and returns its length, delimiter included. `None` if the payload is longer than
/// [`MAX_PAYLOAD`] or `out` is too small.
pub fn encode(kind: u8, seq: u8, payload: &[u8], out: &mut [u8]) -> Option<usize> {
    if payload.len() > MAX_PAYLOAD {
        return None;
    }
    let mut raw = [0u8; MAX_RAW];
    raw[0] = kind;
    raw[1] = seq;
    raw[2..2 + payload.len()].copy_from_slice(payload);
    let n = 2 + payload.len();
    let crc = crc16(&raw[..n]);
    raw[n..n + 2].copy_from_slice(&crc.to_le_bytes());
    let len = cobs::encode(&raw[..n + 2], out)?;
    *out.get_mut(len)? = 0;
    Some(len + 1)
}

/// Finds frames in a byte stream, one byte at a time, with a fixed buffer.
pub struct Decoder {
    buf: [u8; MAX_RAW + 1],
    len: usize,
    overflowed: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Decoder { buf: [0; MAX_RAW + 1], len: 0, overflowed: false }
    }
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    /// Takes one byte. Returns a frame, or why one was dropped, when the byte ends a frame.
    /// Empty frames (two `0x00` in a row) are ignored: a sender may use them to flush.
    pub fn push(&mut self, byte: u8) -> Option<Result<Frame<'_>, BadFrame>> {
        if byte != 0 {
            if self.len < self.buf.len() {
                self.buf[self.len] = byte;
                self.len += 1;
            } else {
                self.overflowed = true;
            }
            return None;
        }
        let len = core::mem::take(&mut self.len);
        if core::mem::take(&mut self.overflowed) {
            return Some(Err(BadFrame::TooLong));
        }
        if len == 0 {
            return None;
        }
        Some(self.check(len))
    }

    fn check(&mut self, len: usize) -> Result<Frame<'_>, BadFrame> {
        let n = cobs::decode_in_place(&mut self.buf, len).map_err(|_| BadFrame::Cobs)?;
        if n < 4 {
            return Err(BadFrame::TooShort);
        }
        if n > MAX_RAW {
            return Err(BadFrame::TooLong);
        }
        let body = n - 2;
        let sent = u16::from_le_bytes([self.buf[body], self.buf[body + 1]]);
        if crc16(&self.buf[..body]) != sent {
            return Err(BadFrame::Crc);
        }
        Ok(Frame { kind: self.buf[0], seq: self.buf[1], payload: &self.buf[2..body] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn wire(kind: u8, seq: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = [0u8; MAX_WIRE];
        let n = encode(kind, seq, payload, &mut out).unwrap();
        out[..n].to_vec()
    }

    /// A decoded frame as owned bytes: kind, seq, payload.
    type Owned = (u8, u8, Vec<u8>);

    /// Every frame or error the decoder reports for `bytes`.
    fn run(bytes: &[u8]) -> Vec<Result<Owned, BadFrame>> {
        let mut d = Decoder::new();
        let mut out = vec![];
        for &b in bytes {
            if let Some(r) = d.push(b) {
                out.push(r.map(|f| (f.kind, f.seq, f.payload.to_vec())));
            }
        }
        out
    }

    #[test]
    fn a_known_frame_on_the_wire() {
        // HEARTBEAT (0x10), seq 7, payload [0x00]: raw 10 07 00 + CRC.
        let crc = crc16(&[0x10, 0x07, 0x00]).to_le_bytes();
        let w = wire(0x10, 7, &[0x00]);
        assert_eq!(w, [0x03, 0x10, 0x07, 0x03, crc[0], crc[1], 0x00]);
    }

    #[test]
    fn the_largest_frame_fits_and_a_larger_payload_is_refused() {
        let payload = [0xAB; MAX_PAYLOAD];
        let w = wire(1, 2, &payload);
        assert!(w.len() <= MAX_WIRE);
        assert_eq!(run(&w), [Ok((1, 2, payload.to_vec()))]);
        assert_eq!(encode(1, 2, &[0; MAX_PAYLOAD + 1], &mut [0; 512]), None);
        assert_eq!(encode(1, 2, &[], &mut [0; 3]), None, "the buffer is too small");
    }

    #[test]
    fn bad_frames_are_reported_and_the_next_one_is_found() {
        let good = wire(0x20, 1, &[1, 2, 3]);
        let mut flipped = good.clone();
        flipped[2] ^= 0x10;
        let mut stream = flipped;
        stream.extend([0x02, 0x05, 0x00]); // too short
        stream.extend([0x05, 0x11, 0x00]); // invalid COBS
        stream.extend(vec![0x55; 400]); // garbage longer than any frame
        stream.push(0);
        stream.extend(&good);
        assert_eq!(
            run(&stream),
            [
                Err(BadFrame::Crc),
                Err(BadFrame::TooShort),
                Err(BadFrame::Cobs),
                Err(BadFrame::TooLong),
                Ok((0x20, 1, vec![1, 2, 3])),
            ]
        );
    }

    #[test]
    fn empty_frames_are_ignored() {
        let mut stream = vec![0, 0, 0];
        stream.extend(wire(3, 4, &[]));
        assert_eq!(run(&stream), [Ok((3, 4, vec![]))]);
    }

    /// Exhaustive: every single flipped bit in a few frames is dropped, and the next frame still
    /// comes through. (Random flips are not used here: a flip that splits a frame in two could,
    /// once in 65 536, give a fragment with a matching CRC, and a test must not be flaky.)
    #[test]
    fn every_single_flipped_bit_is_dropped() {
        let payloads: [&[u8]; 4] = [&[], &[0], &[1, 0, 2, 0, 0, 3], &[0xFF; 60]];
        for payload in payloads {
            let w = wire(0x21, 5, payload);
            for i in 0..(w.len() - 1) * 8 {
                let mut bad = w.clone();
                bad[i / 8] ^= 1 << (i % 8);
                bad.extend(&w);
                let out = run(&bad);
                let original = Ok((0x21, 5, payload.to_vec()));
                assert!(out.iter().all(|r| r.is_err() || *r == original), "bit {i}: {out:?}");
                assert_eq!(out.iter().filter(|r| r.is_ok()).count(), 1, "bit {i}: {out:?}");
                assert_eq!(out.last(), Some(&original));
            }
        }
    }

    proptest! {
        #[test]
        fn encode_then_decode_is_the_identity(
            kind in any::<u8>(), seq in any::<u8>(),
            payload in proptest::collection::vec(any::<u8>(), 0..=MAX_PAYLOAD),
        ) {
            let w = wire(kind, seq, &payload);
            prop_assert_eq!(w.iter().filter(|&&b| b == 0).count(), 1);
            prop_assert_eq!(*w.last().unwrap(), 0);
            prop_assert_eq!(run(&w), vec![Ok((kind, seq, payload))]);
        }

        /// Garbage never panics, and the frame after the next `0x00` always comes through.
        #[test]
        fn garbage_never_panics_and_resyncs_at_the_next_zero(
            junk in proptest::collection::vec(any::<u8>(), 0..700),
            payload in proptest::collection::vec(any::<u8>(), 0..40),
        ) {
            let mut stream = junk;
            stream.push(0);
            stream.extend(wire(9, 9, &payload));
            let out = run(&stream);
            prop_assert_eq!(out.last(), Some(&Ok((9, 9, payload))));
        }
    }
}
