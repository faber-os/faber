//! Little-endian readers and writers for payloads. Strings are `str8`: a length byte, then that
//! many bytes of UTF-8.

use crate::DecodeError;

pub(crate) struct Writer<'a> {
    pub out: &'a mut Vec<u8>,
}

impl Writer<'_> {
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.out.push(v as u8);
    }
    pub fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    /// A `str8`. Longer strings are an encoding error, not silently cut.
    pub fn str8(&mut self, s: &str, field: &'static str) -> Result<(), crate::EncodeError> {
        let len = u8::try_from(s.len()).map_err(|_| crate::EncodeError::TooLong { field })?;
        self.out.push(len);
        self.out.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.bytes.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }
    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self, field: &'static str) -> Result<bool, DecodeError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::BadValue { field }),
        }
    }
    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn f32(&mut self) -> Result<f32, DecodeError> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("took 8 bytes")))
    }
    /// How many bytes are left.
    pub fn remaining(&self) -> usize {
        self.bytes.len()
    }
    pub fn str8(&mut self, field: &'static str) -> Result<String, DecodeError> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| DecodeError::BadValue { field })
    }
    /// Everything that is left.
    pub fn rest(&mut self) -> &'a [u8] {
        core::mem::take(&mut self.bytes)
    }
    /// Fails if bytes are left over: within one protocol version every layout is exact.
    pub fn end(&self) -> Result<(), DecodeError> {
        if self.bytes.is_empty() { Ok(()) } else { Err(DecodeError::TrailingBytes) }
    }
}
