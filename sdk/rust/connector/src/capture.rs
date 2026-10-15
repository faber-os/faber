//! The `.fbcap` capture format: raw bytes from a port, with the time each chunk arrived
//! (docs/api.md, "Capture files"). Pure encoding and decoding; the caller does the file I/O.

use faber_capabilities::Timestamp;

pub const MAGIC: &[u8; 10] = b"FABERCAP1\n";

/// One chunk as it was read from the port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub t_us: Timestamp,
    pub bytes: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// The data does not start with the `.fbcap` magic.
    NotACapture,
}

/// Decodes a whole capture. A truncated last chunk (a recording that was cut off) is dropped.
pub fn decode(data: &[u8]) -> Result<Vec<Chunk>, CaptureError> {
    let body = data.strip_prefix(MAGIC.as_slice()).ok_or(CaptureError::NotACapture)?;
    let mut chunks = Vec::new();
    let mut i = 0;
    while i + 12 <= body.len() {
        let t_us = u64::from_le_bytes(body[i..i + 8].try_into().expect("8 bytes"));
        let len = u32::from_le_bytes(body[i + 8..i + 12].try_into().expect("4 bytes")) as usize;
        i += 12;
        let Some(bytes) = body.get(i..i + len) else { break };
        chunks.push(Chunk { t_us, bytes: bytes.to_vec() });
        i += len;
    }
    Ok(chunks)
}

/// Encodes chunks as a capture.
pub fn encode(chunks: &[Chunk]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for c in chunks {
        out.extend_from_slice(&c.t_us.to_le_bytes());
        out.extend_from_slice(&(c.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&c.bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Chunk> {
        vec![Chunk { t_us: 10, bytes: vec![1, 2, 3] }, Chunk { t_us: 2_000, bytes: vec![] }]
    }

    #[test]
    fn round_trips() {
        assert_eq!(decode(&encode(&sample())).unwrap(), sample());
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(decode(b"SENNEPLDR1\n"), Err(CaptureError::NotACapture));
    }

    #[test]
    fn drops_a_truncated_tail() {
        let mut data = encode(&sample());
        data.extend_from_slice(&5u64.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&[9, 9]); // only 2 of 100 bytes
        assert_eq!(decode(&data).unwrap(), sample());
    }
}
