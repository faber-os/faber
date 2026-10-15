//! LDROBOT LD19 / STL-19P packet protocol (from SENNEP V2's `lidar/` crate, same author).
//!
//! One packet is 47 bytes (dev manual §3.1):
//! `0x54 0x2C | speed u16 (deg/s) | start angle u16 (0.01 deg) | 12 x (distance u16 mm, intensity u8)
//!  | end angle u16 (0.01 deg) | timestamp u16 (ms, wraps at 30000) | CRC8`
//! All multi-byte fields are little endian. Angles grow clockwise seen from above (manual §4);
//! conversion to the robot frame happens in `scan`, not here.

pub const HEADER: u8 = 0x54;
pub const VER_LEN: u8 = 0x2C;
pub const PACKET_LEN: usize = 47;
pub const POINTS_PER_PACKET: usize = 12;
/// The sensor's millisecond timestamp wraps at this value.
pub const TIMESTAMP_WRAP_MS: u32 = 30_000;
/// A packet spans ~8 deg at 10 Hz. Anything wider is corrupt even if the CRC matched by chance.
const MAX_SPAN_CDEG: u32 = 2_000;

const fn make_crc_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x4D } else { crc << 1 };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static CRC_TABLE: [u8; 256] = make_crc_table();

pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |crc, &b| CRC_TABLE[(crc ^ b) as usize])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawPoint {
    pub dist_mm: u16,
    pub intensity: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Packet {
    pub speed_dps: u16,
    pub start_cdeg: u16,
    pub end_cdeg: u16,
    pub timestamp_ms: u16,
    pub points: [RawPoint; POINTS_PER_PACKET],
    /// Host monotonic time (µs) when the chunk holding the packet's last byte was read.
    pub host_us: u64,
}

impl Packet {
    /// Angular width from first to last point, handling the 360 -> 0 wrap.
    pub fn span_cdeg(&self) -> u32 {
        (self.end_cdeg as u32 + 36_000 - self.start_cdeg as u32) % 36_000
    }

    /// Clockwise sensor angle of point `i` in degrees, in [0, 360).
    pub fn angle_deg(&self, i: usize) -> f32 {
        let step = self.span_cdeg() as f32 / (POINTS_PER_PACKET - 1) as f32;
        let a = (self.start_cdeg as f32 + step * i as f32) / 100.0;
        if a >= 360.0 { a - 360.0 } else { a }
    }

    pub fn decode(bytes: &[u8; PACKET_LEN], host_us: u64) -> Packet {
        let u16le = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
        let mut points = [RawPoint::default(); POINTS_PER_PACKET];
        for (i, p) in points.iter_mut().enumerate() {
            let o = 6 + i * 3;
            *p = RawPoint { dist_mm: u16le(o), intensity: bytes[o + 2] };
        }
        Packet {
            speed_dps: u16le(2),
            start_cdeg: u16le(4),
            end_cdeg: u16le(42),
            timestamp_ms: u16le(44),
            points,
            host_us,
        }
    }

    #[cfg(test)]
    pub fn encode(&self) -> [u8; PACKET_LEN] {
        let mut b = [0u8; PACKET_LEN];
        b[0] = HEADER;
        b[1] = VER_LEN;
        b[2..4].copy_from_slice(&self.speed_dps.to_le_bytes());
        b[4..6].copy_from_slice(&self.start_cdeg.to_le_bytes());
        for (i, p) in self.points.iter().enumerate() {
            let o = 6 + i * 3;
            b[o..o + 2].copy_from_slice(&p.dist_mm.to_le_bytes());
            b[o + 2] = p.intensity;
        }
        b[42..44].copy_from_slice(&self.end_cdeg.to_le_bytes());
        b[44..46].copy_from_slice(&self.timestamp_ms.to_le_bytes());
        b[46] = crc8(&b[..46]);
        b
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParserStats {
    pub packets: u64,
    pub crc_errors: u64,
    pub rejected: u64,
    pub skipped_bytes: u64,
}

/// Streaming parser: feed it arbitrary chunks, get whole packets out. Resynchronises after
/// garbage or CRC errors without losing a packet that starts inside the bad bytes.
pub struct Parser {
    buf: [u8; PACKET_LEN],
    fill: usize,
    pub stats: ParserStats,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser { buf: [0; PACKET_LEN], fill: 0, stats: ParserStats::default() }
    }

    pub fn push(&mut self, bytes: &[u8], host_us: u64, out: &mut impl FnMut(Packet)) {
        for &b in bytes {
            self.push_byte(b, host_us, out);
        }
    }

    fn push_byte(&mut self, b: u8, host_us: u64, out: &mut impl FnMut(Packet)) {
        match self.fill {
            0 => {
                if b == HEADER {
                    self.buf[0] = b;
                    self.fill = 1;
                } else {
                    self.stats.skipped_bytes += 1;
                }
            }
            1 => {
                if b == VER_LEN {
                    self.buf[1] = b;
                    self.fill = 2;
                } else if b == HEADER {
                    self.stats.skipped_bytes += 1; // previous 0x54 was noise; this one may start a packet
                } else {
                    self.stats.skipped_bytes += 2;
                    self.fill = 0;
                }
            }
            _ => {
                self.buf[self.fill] = b;
                self.fill += 1;
                if self.fill == PACKET_LEN {
                    self.fill = 0;
                    self.finish(host_us, out);
                }
            }
        }
    }

    fn finish(&mut self, host_us: u64, out: &mut impl FnMut(Packet)) {
        let bytes = self.buf;
        if crc8(&bytes[..PACKET_LEN - 1]) == bytes[PACKET_LEN - 1] {
            let p = Packet::decode(&bytes, host_us);
            if p.span_cdeg() <= MAX_SPAN_CDEG && p.start_cdeg < 36_000 && p.end_cdeg < 36_000 {
                self.stats.packets += 1;
                out(p);
                return;
            }
            self.stats.rejected += 1;
        } else {
            self.stats.crc_errors += 1;
        }
        // Bad packet: a real header may hide inside it. Re-feed everything after the first byte.
        self.stats.skipped_bytes += 1;
        for &b in &bytes[1..] {
            self.push_byte(b, host_us, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Example packet from the LD19 development manual §3.3.
    const MANUAL_EXAMPLE: [u8; PACKET_LEN] = [
        0x54, 0x2C, 0x68, 0x08, 0xAB, 0x7E, 0xE0, 0x00, 0xE4, 0xDC, 0x00, 0xE2, 0xD9, 0x00, 0xE5, 0xD5, 0x00,
        0xE3, 0xD3, 0x00, 0xE4, 0xD0, 0x00, 0xE9, 0xCD, 0x00, 0xE4, 0xCA, 0x00, 0xE2, 0xC7, 0x00, 0xE9, 0xC5,
        0x00, 0xE5, 0xC2, 0x00, 0xE5, 0xC0, 0x00, 0xE5, 0xBE, 0x82, 0x3A, 0x1A, 0x50,
    ];

    #[test]
    fn crc_table_matches_manual() {
        // First entries of the table printed in the manual.
        assert_eq!(&CRC_TABLE[..8], &[0x00, 0x4d, 0x9a, 0xd7, 0x79, 0x34, 0xe3, 0xae]);
        assert_eq!(CRC_TABLE[255], 0xa8);
    }

    #[test]
    fn decodes_manual_example() {
        assert_eq!(crc8(&MANUAL_EXAMPLE[..46]), MANUAL_EXAMPLE[46]);
        let p = Packet::decode(&MANUAL_EXAMPLE, 0);
        assert_eq!(p.speed_dps, 2152);
        assert_eq!(p.start_cdeg, 32427);
        assert_eq!(p.end_cdeg, 33470);
        assert_eq!(p.points[0], RawPoint { dist_mm: 224, intensity: 228 });
        assert_eq!(p.points[1], RawPoint { dist_mm: 220, intensity: 226 });
        assert_eq!(p.points[11].dist_mm, 192);
        assert_eq!(p.timestamp_ms, 0x1A3A);
        assert!((p.angle_deg(0) - 324.27).abs() < 1e-3);
        assert!((p.angle_deg(11) - 334.70).abs() < 1e-3);
        assert_eq!(p.encode(), MANUAL_EXAMPLE);
    }

    #[test]
    fn angle_wraps_across_zero() {
        let mut p = Packet::decode(&MANUAL_EXAMPLE, 0);
        p.start_cdeg = 35_800;
        p.end_cdeg = 300; // 5 deg span across 0
        assert_eq!(p.span_cdeg(), 500);
        assert!((p.angle_deg(0) - 358.0).abs() < 1e-3);
        assert!(p.angle_deg(11) < 3.01 && p.angle_deg(11) > 2.99);
    }

    #[test]
    fn resyncs_after_garbage_and_split_chunks() {
        let mut stream = vec![0x54, 0x00, 0x54, 0x54, 0x2C, 0x01]; // fake headers + a truncated packet
        stream.extend_from_slice(&MANUAL_EXAMPLE);
        let mut corrupt = MANUAL_EXAMPLE;
        corrupt[20] ^= 0xFF;
        stream.extend_from_slice(&corrupt);
        stream.extend_from_slice(&MANUAL_EXAMPLE);
        let mut parser = Parser::new();
        let mut got = Vec::new();
        for chunk in stream.chunks(5) {
            parser.push(chunk, 7, &mut |p| got.push(p));
        }
        // The truncated packet swallows the start of the first real one, so the first real packet
        // only survives if the parser rescans the failed bytes.
        assert_eq!(got.len(), 2, "{:?}", parser.stats);
        assert!(got.iter().all(|p| p.speed_dps == 2152 && p.host_us == 7));
        assert!(parser.stats.crc_errors >= 1);
    }
}
