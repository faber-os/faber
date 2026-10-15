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
}

impl PortId {
    pub fn parse(s: &str) -> Option<PortId> {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for s in
            ["usb:10c4:ea60", "usb:10c4:ea60:0001", "serial:COM3", "serial:/dev/ttyAMA0", "replay:demo-ld19"]
        {
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
}
