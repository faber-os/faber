//! A safe I²C scan, the way `i2cdetect` looks for devices (docs/architecture.md §10, "Detect,
//! then suggest"), but stricter: it only ever *reads*. Never started by Faber itself, only when
//! someone presses Scan.
//!
//! - Addresses 0x08–0x77 only: like `i2cdetect`, it skips 0x00–0x07 and 0x78–0x7f, which the I²C
//!   specification reserves (general call, CBUS, high-speed and 10-bit modes).
//! - Each address gets a one-byte read. `i2cdetect` uses an SMBus "quick write" for most
//!   addresses; a write, even of nothing, can change some chips' state, so Faber never writes to
//!   a device it doesn't know.
//! - An address a part uses isn't probed at all: it is shown as that part's (`i2cdetect`'s `UU`).

use super::buses::I2cBus;

/// The first and last address a scan probes.
pub const FIRST: u8 = 0x08;
pub const LAST: u8 = 0x77;

/// What a scan saw at one address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// Something answered.
    Found,
    /// A part uses it, so it wasn't probed.
    Used(String),
}

/// Probes every address from [`FIRST`] to [`LAST`] with a one-byte read, except those `used`
/// names (address, who). Returns what answered or is in use, lowest address first.
pub fn scan(bus: &mut dyn I2cBus, used: &[(u8, String)]) -> Vec<(u8, Seen)> {
    let mut out = Vec::new();
    for address in FIRST..=LAST {
        if let Some((_, who)) = used.iter().find(|(a, _)| *a == address) {
            out.push((address, Seen::Used(who.clone())));
            continue;
        }
        let mut byte = [0u8; 1];
        if bus.read(address, &mut byte).is_ok() {
            out.push((address, Seen::Found));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Hardware;
    use crate::ports::fake::{FakeHardware, Registers, Tmp102};
    use std::io;

    /// A bus that fails the test on any write, and remembers what was read.
    struct ReadOnly(Vec<u8>, Box<dyn I2cBus>);

    impl I2cBus for ReadOnly {
        fn write_read(&mut self, a: u8, _: &[u8], _: &mut [u8]) -> io::Result<()> {
            panic!("the scan wrote to 0x{a:02x}")
        }
        fn write(&mut self, a: u8, _: &[u8]) -> io::Result<()> {
            panic!("the scan wrote to 0x{a:02x}")
        }
        fn read(&mut self, a: u8, buf: &mut [u8]) -> io::Result<()> {
            self.0.push(a);
            self.1.read(a, buf)
        }
    }

    #[test]
    fn it_only_reads_skips_the_reserved_ranges_and_never_probes_a_parts_address() {
        let hw = FakeHardware::new()
            .with_i2c(1, 0x48, Tmp102::new(|| 20.0))
            .with_i2c(1, 0x49, Tmp102::new(|| 20.0))
            .with_i2c(1, 0x76, Registers::new([(0xd0, 0x60)]))
            .with_i2c(1, 0x03, Registers::new([]))
            .with_i2c(1, 0x7a, Registers::new([]));
        let mut bus = ReadOnly(Vec::new(), hw.open_i2c(1).unwrap());
        let seen = scan(&mut bus, &[(0x49, "cabin_temp".into())]);
        assert_eq!(
            seen,
            [(0x48, Seen::Found), (0x49, Seen::Used("cabin_temp".into())), (0x76, Seen::Found)],
            "0x03 and 0x7a are in the reserved ranges, so they are never seen"
        );
        assert_eq!(bus.0.first(), Some(&0x08));
        assert_eq!(bus.0.last(), Some(&0x77));
        assert!(!bus.0.contains(&0x49), "a part's address isn't touched");
        assert_eq!(bus.0.len(), 0x77 - 0x08, "every other address once");
    }
}
