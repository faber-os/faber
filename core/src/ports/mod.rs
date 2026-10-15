//! Layer 2: the port manager (docs/architecture.md §10). Owns every port: lists them, matches
//! them to stable ids, hands out claims and refuses conflicts, and opens them as byte streams.
//! Nothing else in Faber opens a port.

mod catalog;
mod claims;
mod id;
mod replay;
mod stream;

pub use catalog::{Discovered, PortEntry, PortKind, Usb, catalog};
pub use claims::{ClaimError, Claimant, Claims};
pub use id::PortId;
pub use replay::{DEMO_LD19, Pacing, Replay, ReplayEntry, ReplayLibrary};
pub use stream::{PortEvent, PortOpener, PortReader, PortWriter, SystemOpener, spawn_reader};

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
        ports
            .into_iter()
            .map(|p| Discovered {
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
