//! Intel HEX, the file the Arduino IDE and arduino-cli make (`sketch.ino.hex`).
//!
//! Every line is a record: `:` then hex digits for the byte count, a 16-bit address, the record
//! type, the data, and a checksum that makes all the bytes add up to zero. Types 00 (data), 01
//! (end of file), 02 (extended segment address) and 04 (extended linear address) place the
//! data; 03 and 05 (start address) mean nothing to an AVR and are skipped.

use std::fmt;

/// The highest address a file may reach: well above the biggest AVR (256 KB), small enough that
/// a broken file can't make Faber reserve gigabytes.
pub const MAX_ADDRESS: u32 = 16 << 20;

/// A program to put on a board: its bytes from address 0, with the addresses the file didn't
/// fill set to `0xFF` (erased flash).
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    data: Vec<u8>,
    /// Which addresses the file gave a value.
    defined: Vec<bool>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({} bytes, {} defined)", self.data.len(), self.defined_bytes())
    }
}

/// Why a file isn't a usable Intel HEX file. `line` counts from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HexError {
    /// A line that doesn't start with `:`, or holds something other than hex digits.
    NotHex { line: usize },
    /// A record whose length doesn't match its byte count.
    BadLength { line: usize },
    /// The bytes of a record don't add up: the file was damaged or edited by hand.
    BadChecksum { line: usize },
    /// A record type Intel HEX doesn't have.
    UnknownRecord { line: usize, kind: u8 },
    /// Two records put a value at the same address.
    Overlap { line: usize, address: u32 },
    /// An address beyond [`MAX_ADDRESS`].
    TooFar { line: usize, address: u32 },
    /// No end-of-file record: the file was probably cut off.
    NoEnd,
    /// No data at all.
    Empty,
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::NotHex { line } => write!(f, "line {line} isn't an Intel HEX record"),
            HexError::BadLength { line } => write!(f, "line {line} is shorter or longer than it says"),
            HexError::BadChecksum { line } => {
                write!(f, "line {line} is damaged (its checksum doesn't match)")
            }
            HexError::UnknownRecord { line, kind } => {
                write!(f, "line {line} has a record type ({kind:02X}) Intel HEX doesn't have")
            }
            HexError::Overlap { line, address } => {
                write!(f, "line {line} writes address 0x{address:05X} a second time")
            }
            HexError::TooFar { line, address } => {
                write!(f, "line {line} reaches address 0x{address:X}, far beyond any Arduino's flash")
            }
            HexError::NoEnd => write!(f, "the file has no end record, so it was probably cut off"),
            HexError::Empty => write!(f, "the file holds no program"),
        }
    }
}

impl std::error::Error for HexError {}

fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn record_bytes(text: &str, line: usize) -> Result<Vec<u8>, HexError> {
    let digits = text.strip_prefix(':').ok_or(HexError::NotHex { line })?.as_bytes();
    if digits.len() % 2 != 0 {
        return Err(HexError::BadLength { line });
    }
    digits
        .chunks(2)
        .map(|p| match (digit(p[0]), digit(p[1])) {
            (Some(h), Some(l)) => Ok(h << 4 | l),
            _ => Err(HexError::NotHex { line }),
        })
        .collect()
}

impl Image {
    /// Reads an Intel HEX file.
    pub fn parse(text: &str) -> Result<Image, HexError> {
        let mut image = Image { data: Vec::new(), defined: Vec::new() };
        let mut base: u32 = 0;
        for (i, raw) in text.lines().enumerate() {
            let line = i + 1;
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let rec = record_bytes(raw, line)?;
            if rec.len() < 5 || rec.len() != 5 + rec[0] as usize {
                return Err(HexError::BadLength { line });
            }
            if rec.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0 {
                return Err(HexError::BadChecksum { line });
            }
            let offset = u16::from_be_bytes([rec[1], rec[2]]) as u32;
            let data = &rec[4..rec.len() - 1];
            match rec[3] {
                0x00 => {
                    for (k, b) in data.iter().enumerate() {
                        image.put(base + offset + k as u32, *b, line)?;
                    }
                }
                0x01 => {
                    return if image.data.is_empty() { Err(HexError::Empty) } else { Ok(image) };
                }
                0x02 if data.len() == 2 => base = (u16::from_be_bytes([data[0], data[1]]) as u32) << 4,
                0x04 if data.len() == 2 => base = (u16::from_be_bytes([data[0], data[1]]) as u32) << 16,
                0x02 | 0x04 => return Err(HexError::BadLength { line }),
                0x03 | 0x05 => {}
                kind => return Err(HexError::UnknownRecord { line, kind }),
            }
        }
        Err(if image.data.is_empty() { HexError::Empty } else { HexError::NoEnd })
    }

    /// An image of these bytes from address 0, every one of them defined (for tests and tools).
    pub fn from_bytes(bytes: &[u8]) -> Image {
        Image { data: bytes.to_vec(), defined: vec![true; bytes.len()] }
    }

    fn put(&mut self, address: u32, value: u8, line: usize) -> Result<(), HexError> {
        if address >= MAX_ADDRESS {
            return Err(HexError::TooFar { line, address });
        }
        let a = address as usize;
        if a >= self.data.len() {
            self.data.resize(a + 1, 0xFF);
            self.defined.resize(a + 1, false);
        }
        if self.defined[a] {
            return Err(HexError::Overlap { line, address });
        }
        self.data[a] = value;
        self.defined[a] = true;
        Ok(())
    }

    /// Where the program ends: one past its highest address. This is what must fit in the flash.
    pub fn end(&self) -> u32 {
        self.data.len() as u32
    }

    /// How many bytes the file gave a value.
    pub fn defined_bytes(&self) -> u32 {
        self.defined.iter().filter(|d| **d).count() as u32
    }

    /// The bytes from address 0 to [`Image::end`], `0xFF` where the file has none.
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// The start address of every page of `page_bytes` that holds at least one defined byte,
    /// in order: the pages to write.
    pub fn pages(&self, page_bytes: u32) -> Vec<u32> {
        let page = page_bytes as usize;
        self.defined
            .chunks(page)
            .enumerate()
            .filter(|(_, c)| c.iter().any(|d| *d))
            .map(|(i, _)| (i * page) as u32)
            .collect()
    }

    /// One page from `start`, padded with `0xFF` to `page_bytes`.
    pub fn page(&self, start: u32, page_bytes: u32) -> Vec<u8> {
        let (s, n) = (start as usize, page_bytes as usize);
        let mut out = vec![0xFF; n];
        if s < self.data.len() {
            let end = (s + n).min(self.data.len());
            out[..end - s].copy_from_slice(&self.data[s..end]);
        }
        out
    }

    /// Writes the image as Intel HEX: 16 bytes a record, with extended linear address records
    /// above 64 KB, like avr-objcopy. Only defined bytes are written.
    pub fn to_hex(&self) -> String {
        let mut out = String::new();
        let mut upper: u32 = 0;
        let mut i = 0usize;
        while i < self.data.len() {
            if !self.defined[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.data.len()
                && self.defined[i]
                && i - start < 16
                && (i == start || !i.is_multiple_of(16))
            {
                i += 1;
            }
            let address = start as u32;
            if address >> 16 != upper {
                upper = address >> 16;
                out.push_str(&record(0, 0x04, &(upper as u16).to_be_bytes()));
            }
            out.push_str(&record(address as u16, 0x00, &self.data[start..i]));
        }
        out.push_str(&record(0, 0x01, &[]));
        out
    }
}

fn record(address: u16, kind: u8, data: &[u8]) -> String {
    let mut bytes = vec![data.len() as u8];
    bytes.extend_from_slice(&address.to_be_bytes());
    bytes.push(kind);
    bytes.extend_from_slice(data);
    let sum = bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    bytes.push(sum.wrapping_neg());
    let mut s = String::from(":");
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a real Blink for the Uno, as arduino-cli writes it.
    const BLINK: &str = ":100000000C945C000C946E000C946E000C946E00CA\n\
                         :100010000C946E000C946E000C946E000C946E00A8\n\
                         :00000001FF\n";

    #[test]
    fn reads_data_records() {
        let img = Image::parse(BLINK).unwrap();
        assert_eq!(img.end(), 32);
        assert_eq!(&img.bytes()[..4], &[0x0C, 0x94, 0x5C, 0x00]);
        assert_eq!(img.defined_bytes(), 32);
    }

    #[test]
    fn accepts_windows_line_endings_and_blank_lines() {
        let text = BLINK.replace('\n', "\r\n") + "\r\n";
        assert_eq!(Image::parse(&text).unwrap().end(), 32);
    }

    #[test]
    fn a_damaged_byte_fails_the_checksum_with_its_line() {
        let bad = BLINK.replace(":100010000C946E00", ":100010000C946F00");
        assert_eq!(Image::parse(&bad), Err(HexError::BadChecksum { line: 2 }));
    }

    #[test]
    fn a_wrong_checksum_byte_is_caught() {
        let bad = BLINK.replace("6E00CA", "6E00CB");
        assert_eq!(Image::parse(&bad), Err(HexError::BadChecksum { line: 1 }));
    }

    #[test]
    fn a_cut_off_file_says_so() {
        let cut: String = BLINK.lines().take(2).map(|l| format!("{l}\n")).collect();
        assert_eq!(Image::parse(&cut), Err(HexError::NoEnd));
        let half = &BLINK[..30];
        assert_eq!(Image::parse(half), Err(HexError::BadLength { line: 1 }));
    }

    #[test]
    fn rejects_what_isnt_hex() {
        assert_eq!(Image::parse("hello\n"), Err(HexError::NotHex { line: 1 }));
        assert_eq!(Image::parse(":10zz\n"), Err(HexError::NotHex { line: 1 }));
        assert_eq!(Image::parse(""), Err(HexError::Empty));
        assert_eq!(Image::parse(":00000001FF\n"), Err(HexError::Empty));
        assert_eq!(
            Image::parse(":0100000600F9\n:00000001FF\n"),
            Err(HexError::UnknownRecord { line: 1, kind: 6 })
        );
    }

    #[test]
    fn extended_addresses_place_data_above_64k() {
        let img = Image::parse(":020000040001F9\n:02000000AABB99\n:00000001FF\n").unwrap();
        assert_eq!(img.end(), 0x1_0002);
        assert_eq!(img.bytes()[0x1_0000], 0xAA);
        assert_eq!(img.bytes()[0], 0xFF, "a gap is erased flash");
        let seg = Image::parse(":020000021000EC\n:01000000AA55\n:00000001FF\n").unwrap();
        assert_eq!(seg.bytes()[0x1_0000], 0xAA);
    }

    #[test]
    fn overlaps_and_far_addresses_are_refused() {
        let twice = ":01000000AA55\n:01000000BB44\n:00000001FF\n";
        assert_eq!(Image::parse(twice), Err(HexError::Overlap { line: 2, address: 0 }));
        let far = ":02000004FFFFFC\n:01000000AA55\n:00000001FF\n";
        assert!(matches!(Image::parse(far), Err(HexError::TooFar { line: 2, .. })));
    }

    #[test]
    fn pages_only_where_there_is_data() {
        let bytes = vec![0u8; 10];
        let img = Image::parse(":020000040000FA\n:020100001122CA\n:00000001FF\n").unwrap();
        assert_eq!(img.pages(128), vec![0x100], "the empty pages before it aren't written");
        assert_eq!(img.page(0x100, 128)[..3], [0x11, 0x22, 0xFF]);
        assert_eq!(Image::from_bytes(&bytes).pages(4), vec![0, 4, 8]);
    }

    #[test]
    fn written_hex_reads_back_the_same() {
        let mut bytes: Vec<u8> = (0..300u32).map(|i| (i * 7) as u8).collect();
        bytes.resize(70_000, 0x5A);
        let img = Image::from_bytes(&bytes);
        let back = Image::parse(&img.to_hex()).unwrap();
        assert_eq!(back, img);
        assert_eq!(
            Image::parse(&Image::parse(BLINK).unwrap().to_hex()).unwrap(),
            Image::parse(BLINK).unwrap()
        );
    }
}
