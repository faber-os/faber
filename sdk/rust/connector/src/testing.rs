//! Helpers for testing connectors without hardware.

use crate::bus::{GpioLine, I2cDevice, PortAccess, SpiDevice};
use crate::{Context, Health};
use faber_capabilities::Reading;

/// A [`Context`] that records everything a connector does, for assertions in tests.
#[derive(Debug, Default)]
pub struct RecordingContext {
    pub published: Vec<(String, Reading)>,
    pub health: Vec<Health>,
    /// The device lent to the connector ([`Context::port`]), e.g. a fake I²C sensor.
    pub device: Option<Device>,
}

/// A device for [`RecordingContext::device`].
pub enum Device {
    I2c(Box<dyn I2cDevice>),
    Spi(Box<dyn SpiDevice>),
    Gpio(Box<dyn GpioLine>),
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Device::I2c(_) => "Device::I2c",
            Device::Spi(_) => "Device::Spi",
            Device::Gpio(_) => "Device::Gpio",
        })
    }
}

impl RecordingContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recent health the connector reported, if any.
    pub fn last_health(&self) -> Option<&Health> {
        self.health.last()
    }
}

impl Context for RecordingContext {
    fn publish(&mut self, name: &str, reading: Reading) {
        self.published.push((name.to_string(), reading));
    }

    fn set_health(&mut self, health: Health) {
        self.health.push(health);
    }

    fn port(&mut self) -> Option<PortAccess<'_>> {
        Some(match self.device.as_mut()? {
            Device::I2c(d) => PortAccess::I2c(d.as_mut()),
            Device::Spi(d) => PortAccess::Spi(d.as_mut()),
            Device::Gpio(d) => PortAccess::Gpio(d.as_mut()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::laser_scan;

    #[test]
    fn records_publishes_and_health() {
        let mut ctx = RecordingContext::new();
        assert_eq!(ctx.last_health(), None);
        ctx.set_health(Health::Running);
        ctx.publish(
            "laser-scan",
            Reading::LaserScan(laser_scan::Reading::even_bins(1, 0.0, 0.1, 0.1, vec![])),
        );
        assert_eq!(ctx.last_health(), Some(&Health::Running));
        assert_eq!(ctx.published.len(), 1);
        assert_eq!(ctx.published[0].0, "laser-scan");
    }
}
