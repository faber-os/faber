//! Layer 2. Detect, then suggest (docs/principles.md): "this port might be one of these".
//!
//! Suggestions come from the connectors themselves (their manifests' USB hints) and from what a
//! recording was made with. Honest by design: hints say "often comes with", never "this is".

use crate::ports::{PortEntry, PortKind};
use faber_connector::Manifest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub connector: String,
    pub reason: String,
}

/// `recorded_with` is the connector a replay port's capture was made with, if known.
pub fn suggestions(port: &PortEntry, recorded_with: Option<&str>, manifests: &[Manifest]) -> Vec<Suggestion> {
    if port.kind == PortKind::Replay {
        return recorded_with
            .and_then(|id| manifests.iter().find(|m| m.id == id))
            .map(|m| Suggestion {
                connector: m.id.clone(),
                reason: format!("This is a recording made with the {}.", m.name),
            })
            .into_iter()
            .collect();
    }
    let Some(usb) = &port.usb else { return Vec::new() };
    manifests
        .iter()
        .flat_map(|m| {
            m.usb_hints
                .iter()
                .filter(|h| h.vid == usb.vid && h.pid == usb.pid)
                .map(|h| Suggestion { connector: m.id.clone(), reason: h.reason.clone() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{PortId, Usb};

    fn usb(vid: u16, pid: u16) -> PortEntry {
        PortEntry {
            reserved: None,
            id: PortId::Usb { vid, pid, serial: None },
            kind: PortKind::UsbSerial,
            label: "x".into(),
            path: Some("COM3".into()),
            usb: Some(Usb { vid, pid, serial_number: None, manufacturer: None, product: None }),
        }
    }

    fn manifests() -> Vec<Manifest> {
        vec![ldrobot_ld19::manifest()]
    }

    #[test]
    fn cp2102_suggests_the_ld19_honestly() {
        let s = suggestions(&usb(0x10c4, 0xea60), None, &manifests());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].connector, "ldrobot-ld19");
        assert!(s[0].reason.contains("could also be something else"));
    }

    #[test]
    fn unknown_adapters_get_no_guess() {
        assert!(suggestions(&usb(0x1234, 0x5678), None, &manifests()).is_empty());
    }

    #[test]
    fn recordings_suggest_what_they_were_made_with() {
        let p = PortEntry {
            reserved: None,
            id: PortId::Replay("demo-ld19".into()),
            kind: PortKind::Replay,
            label: "x".into(),
            path: None,
            usb: None,
        };
        let s = suggestions(&p, Some("ldrobot-ld19"), &manifests());
        assert_eq!(s[0].connector, "ldrobot-ld19");
        assert!(s[0].reason.starts_with("This is a recording made with the LDROBOT LD19"));
        assert!(suggestions(&p, None, &manifests()).is_empty());
    }
}
