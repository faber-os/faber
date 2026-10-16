//! Turns what the OS reports into the list of ports with stable ids.

use super::PortId;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usb {
    pub vid: u16,
    pub pid: u16,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

/// One serial port as the OS reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovered {
    /// OS name, e.g. `COM3` or `/dev/ttyUSB0`.
    pub path: String,
    pub usb: Option<Usb>,
    /// A serial link over Bluetooth (e.g. Windows' "Standard Serial over Bluetooth link").
    pub bluetooth: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortKind {
    UsbSerial,
    Serial,
    Bluetooth,
    Replay,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortEntry {
    pub id: PortId,
    pub kind: PortKind,
    pub label: String,
    /// The OS name to open, for serial ports.
    pub path: Option<String>,
    pub usb: Option<Usb>,
}

/// Builds the port list. USB adapters get `usb:<vid>:<pid>` so they keep their id when the OS
/// renumbers them (COM3 becomes COM5). Two identical adapters are told apart by serial number; if
/// they have none, they fall back to their OS name, the only thing that differs.
pub fn catalog(discovered: &[Discovered], replays: &[String]) -> Vec<PortEntry> {
    let mut same_model: BTreeMap<(u16, u16), usize> = BTreeMap::new();
    for d in discovered {
        if let Some(u) = &d.usb {
            *same_model.entry((u.vid, u.pid)).or_default() += 1;
        }
    }
    let mut out: Vec<PortEntry> = discovered
        .iter()
        .map(|d| {
            let (id, kind) = match &d.usb {
                Some(u) => {
                    let unique = same_model[&(u.vid, u.pid)] == 1;
                    let id = match (unique, &u.serial_number) {
                        (true, _) => Some(PortId::Usb { vid: u.vid, pid: u.pid, serial: None }),
                        (false, Some(s)) if !s.is_empty() => {
                            Some(PortId::Usb { vid: u.vid, pid: u.pid, serial: Some(s.clone()) })
                        }
                        _ => None,
                    };
                    (id.unwrap_or_else(|| PortId::Serial(d.path.clone())), PortKind::UsbSerial)
                }
                None if d.bluetooth => (PortId::Serial(d.path.clone()), PortKind::Bluetooth),
                None => (PortId::Serial(d.path.clone()), PortKind::Serial),
            };
            let name = d.usb.as_ref().and_then(|u| u.product.clone().or_else(|| u.manufacturer.clone()));
            let label = match name {
                // Windows' names often end in the port already: "Arduino Mega 2560 (COM6)".
                Some(name) => match name.strip_suffix(&format!(" ({})", d.path)) {
                    Some(bare) => format!("{bare} ({})", d.path),
                    None => format!("{name} ({})", d.path),
                },
                None if d.bluetooth => format!("Bluetooth serial link ({})", d.path),
                None => d.path.clone(),
            };
            PortEntry { id, kind, label, path: Some(d.path.clone()), usb: d.usb.clone() }
        })
        .collect();
    out.extend(replays.iter().map(|name| PortEntry {
        id: PortId::Replay(name.clone()),
        kind: PortKind::Replay,
        label: format!("Recording: {name}"),
        path: None,
        usb: None,
    }));
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb(path: &str, vid: u16, pid: u16, serial: Option<&str>) -> Discovered {
        Discovered {
            path: path.into(),
            usb: Some(Usb {
                vid,
                pid,
                serial_number: serial.map(str::to_string),
                manufacturer: Some("Silicon Labs".into()),
                product: Some("CP2102 USB to UART Bridge".into()),
            }),
            bluetooth: false,
        }
    }

    #[test]
    fn a_single_adapter_gets_its_model_id() {
        let c = catalog(&[usb("COM3", 0x10c4, 0xea60, Some("0001"))], &[]);
        assert_eq!(c[0].id.to_string(), "usb:10c4:ea60");
        assert_eq!(c[0].kind, PortKind::UsbSerial);
        assert_eq!(c[0].label, "CP2102 USB to UART Bridge (COM3)");
        assert_eq!(c[0].path.as_deref(), Some("COM3"));
    }

    #[test]
    fn identical_adapters_are_told_apart_by_serial_number() {
        let c =
            catalog(&[usb("COM3", 0x10c4, 0xea60, Some("A")), usb("COM4", 0x10c4, 0xea60, Some("B"))], &[]);
        let ids: Vec<_> = c.iter().map(|e| e.id.to_string()).collect();
        assert_eq!(ids, ["usb:10c4:ea60:A", "usb:10c4:ea60:B"]);
    }

    #[test]
    fn identical_adapters_without_serial_fall_back_to_the_os_name() {
        let c = catalog(&[usb("COM3", 0x1a86, 0x7523, None), usb("COM4", 0x1a86, 0x7523, None)], &[]);
        let ids: Vec<_> = c.iter().map(|e| e.id.to_string()).collect();
        assert_eq!(ids, ["serial:COM3", "serial:COM4"]);
    }

    #[test]
    fn a_name_that_already_ends_in_its_port_isnt_given_it_twice() {
        // Windows' name for an Arduino is "Arduino Mega 2560 (COM6)" already.
        let mut d = usb("COM6", 0x2341, 0x0042, None);
        d.usb.as_mut().unwrap().product = Some("Arduino Mega 2560 (COM6)".into());
        assert_eq!(catalog(&[d], &[])[0].label, "Arduino Mega 2560 (COM6)");
    }

    #[test]
    fn bluetooth_links_say_so() {
        let c = catalog(&[Discovered { path: "COM10".into(), usb: None, bluetooth: true }], &[]);
        assert_eq!(c[0].kind, PortKind::Bluetooth);
        assert_eq!(c[0].label, "Bluetooth serial link (COM10)");
        assert_eq!(c[0].id.to_string(), "serial:COM10");
    }

    #[test]
    fn plain_uarts_and_replays() {
        let c = catalog(
            &[Discovered { path: "/dev/ttyAMA0".into(), usb: None, bluetooth: false }],
            &["demo-ld19".into()],
        );
        assert_eq!(c.len(), 2, "real ports are listed before recordings");
        assert_eq!(c[0].id.to_string(), "serial:/dev/ttyAMA0");
        assert_eq!(c[0].label, "/dev/ttyAMA0");
        assert_eq!(c[1].id.to_string(), "replay:demo-ld19");
        assert_eq!(c[1].kind, PortKind::Replay);
    }
}
