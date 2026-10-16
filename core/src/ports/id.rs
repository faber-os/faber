//! Stable port ids, as written in parts.toml (docs/api.md, "Ports").

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PortId {
    /// `usb:<vid>:<pid>` or, when two identical adapters are present, `usb:<vid>:<pid>:<serial>`.
    Usb { vid: u16, pid: u16, serial: Option<String> },
    /// `serial:<os name>`, e.g. `serial:COM3` or `serial:/dev/ttyAMA0`.
    Serial(String),
    /// `replay:<name>`: a capture played back as if it were live.
    Replay(String),
    /// `i2c-<bus>`: an I²C bus (`/dev/i2c-<bus>`), shared by parts at different addresses.
    I2c(u32),
    /// `spi-<bus>.<cs>`: one SPI device, a bus and a chip select (`/dev/spidev<bus>.<cs>`).
    Spi { bus: u32, cs: u32 },
    /// `gpio-<line>`: one GPIO line on the header chip, by its number (GPIO17 is `gpio-17`).
    Gpio(u32),
}

/// A plain number: digits only, no sign, no leading zero (so each id has one spelling).
fn number(s: &str) -> Option<u32> {
    let canonical =
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    if canonical { s.parse().ok() } else { None }
}

impl PortId {
    /// The bus kinds' ids (`i2c-1`, `spi-0.0`, `gpio-17`) have no colon: they are the names the
    /// Pi itself uses (docs/glossary.md).
    fn parse_bus(s: &str) -> Option<PortId> {
        if let Some(n) = s.strip_prefix("i2c-") {
            return number(n).map(PortId::I2c);
        }
        if let Some(rest) = s.strip_prefix("spi-") {
            let (bus, cs) = rest.split_once('.')?;
            return Some(PortId::Spi { bus: number(bus)?, cs: number(cs)? });
        }
        s.strip_prefix("gpio-").and_then(number).map(PortId::Gpio)
    }

    /// True for the bus kinds: I²C, SPI and GPIO.
    pub fn is_bus(&self) -> bool {
        matches!(self, PortId::I2c(_) | PortId::Spi { .. } | PortId::Gpio(_))
    }

    pub fn parse(s: &str) -> Option<PortId> {
        if let Some(id) = PortId::parse_bus(s) {
            return Some(id);
        }
        let (kind, rest) = s.split_once(':')?;
        match kind {
            "usb" => {
                let mut it = rest.splitn(3, ':');
                let vid = u16::from_str_radix(it.next()?, 16).ok()?;
                let pid = u16::from_str_radix(it.next()?, 16).ok()?;
                let serial = it.next().map(str::to_string);
                if serial.as_deref() == Some("") {
                    return None;
                }
                Some(PortId::Usb { vid, pid, serial })
            }
            "serial" if !rest.is_empty() => Some(PortId::Serial(rest.to_string())),
            "replay" if !rest.is_empty() => Some(PortId::Replay(rest.to_string())),
            _ => None,
        }
    }
}

impl fmt::Display for PortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortId::Usb { vid, pid, serial: None } => write!(f, "usb:{vid:04x}:{pid:04x}"),
            PortId::Usb { vid, pid, serial: Some(s) } => write!(f, "usb:{vid:04x}:{pid:04x}:{s}"),
            PortId::Serial(p) => write!(f, "serial:{p}"),
            PortId::Replay(n) => write!(f, "replay:{n}"),
            PortId::I2c(bus) => write!(f, "i2c-{bus}"),
            PortId::Spi { bus, cs } => write!(f, "spi-{bus}.{cs}"),
            PortId::Gpio(line) => write!(f, "gpio-{line}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for s in [
            "usb:10c4:ea60",
            "usb:10c4:ea60:0001",
            "serial:COM3",
            "serial:/dev/ttyAMA0",
            "replay:demo-ld19",
            "i2c-1",
            "i2c-0",
            "spi-0.1",
            "gpio-17",
        ] {
            assert_eq!(PortId::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn usb_ids_are_hex_and_written_lowercase() {
        assert_eq!(
            PortId::parse("usb:10C4:EA60").unwrap(),
            PortId::Usb { vid: 0x10c4, pid: 0xea60, serial: None }
        );
        assert_eq!(PortId::parse("usb:10C4:EA60").unwrap().to_string(), "usb:10c4:ea60");
    }

    #[test]
    fn rejects_nonsense() {
        for s in ["", "COM3", "usb:xyz:1", "usb:10c4", "usb:10c4:ea60:", "serial:", "replay:", "tcp:1.2.3.4"]
        {
            assert_eq!(PortId::parse(s), None, "{s}");
        }
    }

    #[test]
    fn bus_ids_are_the_pis_own_names_with_one_spelling() {
        assert_eq!(PortId::parse("i2c-1"), Some(PortId::I2c(1)));
        assert_eq!(PortId::parse("spi-0.1"), Some(PortId::Spi { bus: 0, cs: 1 }));
        assert_eq!(PortId::parse("gpio-17"), Some(PortId::Gpio(17)));
        assert!(PortId::Gpio(4).is_bus() && !PortId::Serial("COM3".into()).is_bus());
        for s in [
            "i2c-",
            "i2c-01",
            "i2c--1",
            "i2c-x",
            "spi-0",
            "spi-0.",
            "spi-.0",
            "spi-0.0.0",
            "gpio-+1",
            "gpio-17a",
        ] {
            assert_eq!(PortId::parse(s), None, "{s}");
        }
    }
}
