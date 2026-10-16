//! Layer 2: the port manager (docs/architecture.md §10). Owns every port: lists them, matches
//! them to stable ids, hands out claims and refuses conflicts, and opens them: serial ports as
//! byte streams, I²C, SPI and GPIO as devices lent to one part ([`Hardware`]). Nothing else in
//! Faber opens a port.

mod buses;
mod catalog;
mod claims;
pub mod fake;
pub mod header;
mod id;
mod linux;
mod replay;
mod stream;
mod usb_problems;

pub use buses::{
    AddressedDevice, BusPort, BusSupport, Hardware, I2cBus, NoHardware, system_hardware, valid_address,
};
pub use catalog::{Discovered, PortEntry, PortKind, Usb, bus_label, catalog};
pub use claims::{ClaimError, Claimant, Claims};
pub use id::PortId;
pub use linux::consumer as gpio_consumer;
pub use replay::{
    DEMO_LD19, Pacing, Replay, ReplayEntry, ReplayLibrary, SAMPLE_LD19_BYTES, SAMPLE_LD19_NAME,
};
pub use stream::{PortEvent, PortOpener, PortReader, PortWriter, SystemOpener, spawn_reader};
pub use usb_problems::{FailedUsb, PortProblem, SystemUsbProblems, UsbProblemDiscovery, port_problem};

/// Lists the serial ports the OS knows about. A trait so tests can fake hardware.
pub trait SerialDiscovery: Send + Sync {
    fn list(&self) -> Vec<Discovered>;
}

/// The real OS discovery via the `serialport` crate.
pub struct SystemDiscovery;

impl SerialDiscovery for SystemDiscovery {
    fn list(&self) -> Vec<Discovered> {
        let ports = match serialport::available_ports() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(target: "core::ports", "could not list serial ports: {e}");
                return Vec::new();
            }
        };
        let os_bluetooth = os_bluetooth_ports();
        ports
            .into_iter()
            .map(|p| Discovered {
                bluetooth: matches!(p.port_type, serialport::SerialPortType::BluetoothPort)
                    || os_bluetooth.contains(&p.port_name),
                path: p.port_name,
                usb: match p.port_type {
                    serialport::SerialPortType::UsbPort(u) => Some(Usb {
                        vid: u.vid,
                        pid: u.pid,
                        serial_number: u.serial_number,
                        manufacturer: u.manufacturer,
                        product: u.product,
                    }),
                    _ => None,
                },
            })
            .collect()
    }
}

/// Which of `(device, port)` pairs from the OS serial map are Bluetooth links. Windows maps
/// `\Device\BthModem<n>` to the COM port of every Bluetooth serial link.
pub fn bluetooth_from_device_map(map: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
    map.into_iter().filter(|(device, _)| device.contains("BthModem")).map(|(_, port)| port).collect()
}

/// The `serialport` crate reports Windows Bluetooth links as an unknown type, so the port map in
/// the registry (`HKLM\HARDWARE\DEVICEMAP\SERIALCOMM`) tells them apart.
#[cfg(windows)]
fn os_bluetooth_ports() -> Vec<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"HARDWARE\DEVICEMAP\SERIALCOMM") else {
        return Vec::new();
    };
    let pairs = key.enum_values().flatten().map(|(name, value)| (name, value.to_string()));
    bluetooth_from_device_map(pairs)
}

#[cfg(not(windows))]
fn os_bluetooth_ports() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bthmodem_devices_are_bluetooth() {
        let map = [
            (r"\Device\BthModem1".to_string(), "COM10".to_string()),
            (r"\Device\Silabser0".to_string(), "COM5".to_string()),
            (r"\Device\BthModem0".to_string(), "COM12".to_string()),
        ];
        assert_eq!(bluetooth_from_device_map(map), ["COM10", "COM12"]);
    }
}
