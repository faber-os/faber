//! Layer 2. Detect, then suggest (docs/principles.md): "this port might be one of these".
//!
//! Suggestions come from the connectors themselves (their manifests' USB hints) and from what a
//! recording was made with. Honest by design: hints say "often comes with", never "this is".

use crate::ports::{PortEntry, PortKind};
use faber_connector::{Manifest, PortRequirement};

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

/// A guess for a device an I²C scan found at an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I2cSuggestion {
    /// What it might be, e.g. "TI TMP102 temperature sensor".
    pub chip: String,
    /// An installed connector for it, when there is one: Add part can use it.
    pub connector: Option<String>,
    pub reason: String,
}

/// Common chips by their usual addresses. Only a guess: other chips use the same addresses, and
/// a scan sees an address, not a chip. The connector is the one Faber's examples have for it.
const COMMON: &[(&[u8], &str, Option<&str>)] = &[
    (&[0x48, 0x49, 0x4a, 0x4b], "TI TMP102 temperature sensor", Some("ti-tmp102")),
    (&[0x76, 0x77], "Bosch BME280 temperature, humidity and pressure sensor", None),
    (&[0x68], "InvenSense MPU-6050 accelerometer and gyroscope", None),
    (&[0x29], "ST VL53L0X distance sensor", None),
    (&[0x3c], "SSD1306 OLED display", None),
];

/// What a device found at `address` might be: common chips with that default address, then any
/// installed connector that can be at it. Honest by design: "a common address for", never "this is".
pub fn i2c_suggestions(address: u8, manifests: &[Manifest]) -> Vec<I2cSuggestion> {
    let installed = |id: &str| manifests.iter().find(|m| m.id == id);
    let mut out: Vec<I2cSuggestion> = COMMON
        .iter()
        .filter(|(addresses, ..)| addresses.contains(&address))
        .map(|(_, chip, connector)| I2cSuggestion {
            chip: (*chip).to_string(),
            connector: connector.and_then(installed).map(|m| m.id.clone()),
            reason: format!(
                "0x{address:02x} is a common default address for the {chip}. Other chips use it too, so the scan can't be sure."
            ),
        })
        .collect();
    for m in manifests {
        let PortRequirement::I2c { addresses, .. } = &m.port else { continue };
        if !addresses.contains(&address) || out.iter().any(|s| s.connector.as_deref() == Some(&m.id)) {
            continue;
        }
        out.push(I2cSuggestion {
            chip: m.name.clone(),
            connector: Some(m.id.clone()),
            reason: format!(
                "The {} connector can be at 0x{address:02x}. The scan only sees that something answers, so it can't be sure.",
                m.name
            ),
        });
    }
    out
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

    fn tmp102_connector() -> Manifest {
        Manifest {
            id: "ti-tmp102".into(),
            name: "TI TMP102 temperature sensor".into(),
            version: "1.0.0".into(),
            port: PortRequirement::I2c { addresses: vec![0x48, 0x49, 0x4a, 0x4b], default: 0x48 },
            provides: vec![],
            settings: vec![],
            usb_hints: vec![],
        }
    }

    #[test]
    fn a_found_address_suggests_common_chips_and_says_it_can_not_be_sure() {
        let s = i2c_suggestions(0x48, &[]);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].chip.as_str(), s[0].connector.as_deref()), ("TI TMP102 temperature sensor", None));
        assert!(s[0].reason.contains("common default address") && s[0].reason.contains("can't be sure"));
        let s = i2c_suggestions(0x49, &[tmp102_connector()]);
        assert_eq!(s.len(), 1, "the installed connector is the same suggestion, not a second one");
        assert_eq!(s[0].connector.as_deref(), Some("ti-tmp102"));
        for (a, chip) in [(0x77, "BME280"), (0x68, "MPU-6050"), (0x29, "VL53L0X"), (0x3c, "SSD1306")] {
            assert!(i2c_suggestions(a, &[])[0].chip.contains(chip), "0x{a:02x}");
        }
        assert!(i2c_suggestions(0x10, &[]).is_empty());
    }

    #[test]
    fn any_installed_i2c_connector_is_suggested_at_its_addresses() {
        let other = Manifest { id: "acme-adc".into(), name: "Acme ADC".into(), ..tmp102_connector() };
        let s = i2c_suggestions(0x4b, &[other]);
        assert_eq!(s.len(), 2);
        assert_eq!((s[1].chip.as_str(), s[1].connector.as_deref()), ("Acme ADC", Some("acme-adc")));
    }
}
