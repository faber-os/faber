//! Buses a connector reaches through the core: one I²C device, one SPI device or one GPIO line.
//!
//! A connector still does no I/O of its own. For a part on an I²C, SPI or GPIO port, the core's
//! port manager opens the part's claimed device (its bus and address, its chip select, its line)
//! and lends the connector a handle to *that device only* through [`Context::port`]
//! (docs/architecture.md §4: "the port manager hands it a handle, not a path"). The connector can't
//! name another address or line, so it can't touch anything it wasn't given.
//!
//! [`Context::port`]: crate::Context::port

use std::fmt;

/// Why a bus transfer failed, in plain words for the part's problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusError {
    pub message: String,
}

impl BusError {
    pub fn new(message: impl Into<String>) -> BusError {
        BusError { message: message.into() }
    }
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BusError {}

/// One device on an I²C bus, at the part's address.
pub trait I2cDevice: Send {
    /// The 7-bit address the part sits at, e.g. `0x48`.
    fn address(&self) -> u8;

    /// Writes `write`, then reads `read.len()` bytes, as one transaction with a repeated start
    /// (how a register is read: write its number, read its value).
    fn write_read(&mut self, write: &[u8], read: &mut [u8]) -> Result<(), BusError>;

    /// Writes `data` (a register number, then the value to put in it).
    fn write(&mut self, data: &[u8]) -> Result<(), BusError>;
}

/// One SPI device: a bus and a chip select.
pub trait SpiDevice: Send {
    /// Clocks `write` out while clocking as many bytes in, which come back in `read`
    /// (`read.len() == write.len()`).
    fn transfer(&mut self, write: &[u8], read: &mut [u8]) -> Result<(), BusError>;
}

/// The pull resistor on an input line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    None,
    PullUp,
    PullDown,
}

/// One GPIO line. Levels here are *physical*: `true` is a high pin. Active-low is the
/// connector's business.
pub trait GpioLine: Send {
    /// The line's number on the header chip, e.g. 17 for GPIO17.
    fn line(&self) -> u32;

    /// Makes the line an input with this pull resistor, watching both edges.
    fn set_input(&mut self, bias: Bias) -> Result<(), BusError>;

    /// Makes the line an output, driving `high` at once.
    fn set_output(&mut self, high: bool) -> Result<(), BusError>;

    /// The level on the pin now.
    fn get(&mut self) -> Result<bool, BusError>;

    /// Drives an output line.
    fn set(&mut self, high: bool) -> Result<(), BusError>;

    /// The levels an input changed to since the last call, oldest first, so no edge between two
    /// ticks is lost. Empty for an output line.
    fn edges(&mut self) -> Result<Vec<bool>, BusError>;
}

/// The device the core lent a running part, by the kind of port it is on.
pub enum PortAccess<'a> {
    I2c(&'a mut dyn I2cDevice),
    Spi(&'a mut dyn SpiDevice),
    Gpio(&'a mut dyn GpioLine),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_error_shows_its_message() {
        let e = BusError::new("no answer at 0x48");
        assert_eq!(e.to_string(), "no answer at 0x48");
    }
}
