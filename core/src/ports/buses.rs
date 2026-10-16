//! The buses: I²C, SPI and GPIO (docs/architecture.md §10). A [`Hardware`] lists them and opens
//! them; the real one is Linux-only ([`super::linux`]), the fake one (`--fake-hardware`) works on
//! any OS ([`super::fake`]), and a computer without either has none ([`NoHardware`]).

use super::PortId;
use faber_connector::bus::{BusError, GpioLine, I2cDevice, SpiDevice};
use std::io;
use std::sync::Arc;

/// Where this machine's buses come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusSupport {
    /// The operating system's own (Linux: `/dev/i2c-*`, `/dev/spidev*`, the GPIO character device).
    System,
    /// Simulated buses with simulated parts on them (`--fake-hardware`, for trying things out).
    Fake,
    /// This operating system gives programs no I²C, SPI or GPIO (Windows, macOS).
    None,
}

/// One bus port the hardware has: an I²C bus, an SPI device or a GPIO line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusPort {
    pub id: PortId,
    /// What the OS calls it, e.g. `/dev/i2c-1`.
    pub path: String,
    /// Set when something outside Faber already uses it (a kernel driver holds the GPIO line),
    /// in words: "used by spi0 CS0".
    pub used_by: Option<String>,
}

/// A whole I²C bus: any address. Only the port manager holds one; a part gets an
/// [`AddressedDevice`] for its own address.
pub trait I2cBus: Send {
    /// Writes, then reads, as one transaction with a repeated start.
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()>;
    fn write(&mut self, address: u8, data: &[u8]) -> io::Result<()>;
    /// A plain read: the I²C scan's probe (it never writes).
    fn read(&mut self, address: u8, read: &mut [u8]) -> io::Result<()>;
}

/// Lists and opens the buses. A trait so tests and `--fake-hardware` can fake them.
pub trait Hardware: Send + Sync {
    fn support(&self) -> BusSupport;
    /// The buses there are right now.
    fn list(&self) -> Vec<BusPort>;
    fn open_i2c(&self, bus: u32) -> io::Result<Box<dyn I2cBus>>;
    fn open_spi(&self, bus: u32, cs: u32) -> io::Result<Box<dyn SpiDevice>>;
    /// Opens one line without changing it; the connector then makes it an input or an output.
    /// `consumer` is what the OS shows as its user (`faber:<part>`).
    fn open_gpio(&self, line: u32, consumer: &str) -> io::Result<Box<dyn GpioLine>>;
}

/// The system's buses: Linux's own, anywhere else none.
pub fn system_hardware() -> Arc<dyn Hardware> {
    #[cfg(target_os = "linux")]
    {
        Arc::new(super::linux::LinuxHardware)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Arc::new(NoHardware)
    }
}

/// A computer without I²C, SPI or GPIO.
pub struct NoHardware;

fn no_buses() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "this computer has no I²C, SPI or GPIO buses")
}

impl Hardware for NoHardware {
    fn support(&self) -> BusSupport {
        BusSupport::None
    }

    fn list(&self) -> Vec<BusPort> {
        Vec::new()
    }

    fn open_i2c(&self, _: u32) -> io::Result<Box<dyn I2cBus>> {
        Err(no_buses())
    }

    fn open_spi(&self, _: u32, _: u32) -> io::Result<Box<dyn SpiDevice>> {
        Err(no_buses())
    }

    fn open_gpio(&self, _: u32, _: &str) -> io::Result<Box<dyn GpioLine>> {
        Err(no_buses())
    }
}

/// One part's device on a bus: the bus, held to the part's address.
pub struct AddressedDevice {
    bus: Box<dyn I2cBus>,
    address: u8,
}

impl AddressedDevice {
    pub fn new(bus: Box<dyn I2cBus>, address: u8) -> AddressedDevice {
        AddressedDevice { bus, address }
    }
}

/// "no answer from 0x48 (…)": what a failed transfer means to the user.
fn transfer_error(address: u8, e: io::Error) -> BusError {
    BusError::new(format!("no answer from the device at 0x{address:02x} ({e})"))
}

impl I2cDevice for AddressedDevice {
    fn address(&self) -> u8 {
        self.address
    }

    fn write_read(&mut self, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.bus.write_read(self.address, write, read).map_err(|e| transfer_error(self.address, e))
    }

    fn write(&mut self, data: &[u8]) -> Result<(), BusError> {
        self.bus.write(self.address, data).map_err(|e| transfer_error(self.address, e))
    }
}

/// The 7-bit addresses an I²C address can be: 0x00–0x7f.
pub fn valid_address(address: u8) -> bool {
    address <= 0x7f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_computer_without_buses_says_so() {
        let h = NoHardware;
        assert_eq!(h.support(), BusSupport::None);
        assert!(h.list().is_empty());
        let e = h.open_i2c(1).err().unwrap();
        assert!(e.to_string().contains("no I²C, SPI or GPIO"), "{e}");
    }

    #[test]
    fn a_part_reaches_only_its_own_address() {
        struct Recorder(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl I2cBus for Recorder {
            fn write_read(&mut self, a: u8, _: &[u8], r: &mut [u8]) -> io::Result<()> {
                self.0.lock().unwrap().push(a);
                r.fill(0xab);
                Ok(())
            }
            fn write(&mut self, a: u8, _: &[u8]) -> io::Result<()> {
                self.0.lock().unwrap().push(a);
                Err(io::Error::other("NACK"))
            }
            fn read(&mut self, _: u8, _: &mut [u8]) -> io::Result<()> {
                unreachable!()
            }
        }
        let seen = std::sync::Arc::default();
        let mut d = AddressedDevice::new(Box::new(Recorder(std::sync::Arc::clone(&seen))), 0x48);
        let mut buf = [0; 2];
        d.write_read(&[0], &mut buf).unwrap();
        assert_eq!(buf, [0xab, 0xab]);
        let e = d.write(&[1, 2]).unwrap_err();
        assert_eq!(e.message, "no answer from the device at 0x48 (NACK)");
        assert_eq!(*seen.lock().unwrap(), [0x48, 0x48]);
        assert!(valid_address(0x77) && !valid_address(0x80));
    }
}
