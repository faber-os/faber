//! The Raspberry Pi's 40-pin header: which physical pin is which GPIO line, and what else it does.
//! The same on every Pi with a 40-pin header (B+ onwards, including the Pi 5), from the Raspberry
//! Pi documentation ("GPIO and the 40-pin header").

/// One pin on the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderPin {
    /// The physical pin, 1–40 (1 is the square pad; odd pins on the inner row).
    pub pin: u8,
    /// What it is: `GPIO17`, `3V3`, `5V`, `GND`.
    pub name: &'static str,
    /// Its GPIO line on the header chip (BCM numbering), for a GPIO pin.
    pub line: Option<u32>,
    /// Its other job when a bus or the UART is on, e.g. `I²C 1 SDA`. Empty: none worth naming.
    pub function: &'static str,
}

const fn gpio(pin: u8, line: u32, name: &'static str, function: &'static str) -> HeaderPin {
    HeaderPin { pin, name, line: Some(line), function }
}

const fn power(pin: u8, name: &'static str) -> HeaderPin {
    HeaderPin { pin, name, line: None, function: "" }
}

/// Every pin, 1 to 40.
pub const HEADER: [HeaderPin; 40] = [
    power(1, "3V3"),
    power(2, "5V"),
    gpio(3, 2, "GPIO2", "I²C 1 SDA"),
    power(4, "5V"),
    gpio(5, 3, "GPIO3", "I²C 1 SCL"),
    power(6, "GND"),
    gpio(7, 4, "GPIO4", ""),
    gpio(8, 14, "GPIO14", "UART TX"),
    power(9, "GND"),
    gpio(10, 15, "GPIO15", "UART RX"),
    gpio(11, 17, "GPIO17", ""),
    gpio(12, 18, "GPIO18", "PWM 0"),
    gpio(13, 27, "GPIO27", ""),
    power(14, "GND"),
    gpio(15, 22, "GPIO22", ""),
    gpio(16, 23, "GPIO23", ""),
    power(17, "3V3"),
    gpio(18, 24, "GPIO24", ""),
    gpio(19, 10, "GPIO10", "SPI 0 MOSI"),
    power(20, "GND"),
    gpio(21, 9, "GPIO9", "SPI 0 MISO"),
    gpio(22, 25, "GPIO25", ""),
    gpio(23, 11, "GPIO11", "SPI 0 SCLK"),
    gpio(24, 8, "GPIO8", "SPI 0 CE0"),
    power(25, "GND"),
    gpio(26, 7, "GPIO7", "SPI 0 CE1"),
    gpio(27, 0, "GPIO0", "HAT EEPROM (ID_SD)"),
    gpio(28, 1, "GPIO1", "HAT EEPROM (ID_SC)"),
    gpio(29, 5, "GPIO5", ""),
    power(30, "GND"),
    gpio(31, 6, "GPIO6", ""),
    gpio(32, 12, "GPIO12", "PWM 0"),
    gpio(33, 13, "GPIO13", "PWM 1"),
    power(34, "GND"),
    gpio(35, 19, "GPIO19", "SPI 1 MISO"),
    gpio(36, 16, "GPIO16", "SPI 1 CE2"),
    gpio(37, 26, "GPIO26", ""),
    gpio(38, 20, "GPIO20", "SPI 1 MOSI"),
    power(39, "GND"),
    gpio(40, 21, "GPIO21", "SPI 1 SCLK"),
];

/// The header pin a GPIO line comes out on.
pub fn pin_of(line: u32) -> Option<&'static HeaderPin> {
    HEADER.iter().find(|p| p.line == Some(line))
}

/// The lines Faber offers as `gpio-<line>` ports: every GPIO on the header except GPIO0 and
/// GPIO1, which belong to the HAT identification EEPROM.
pub fn usable_lines() -> impl Iterator<Item = u32> {
    HEADER.iter().filter_map(|p| p.line).filter(|&l| l >= 2)
}

/// The GPIO lines a bus uses while it is on. Using one of them as a plain GPIO would take it
/// away from the bus (on the Pi, requesting the line switches the pin's function).
pub fn bus_lines(bus: &super::PortId) -> &'static [u32] {
    use super::PortId;
    match bus {
        PortId::I2c(1) => &[2, 3],
        PortId::I2c(0) => &[0, 1],
        PortId::Spi { bus: 0, cs: 0 } => &[8, 9, 10, 11],
        PortId::Spi { bus: 0, cs: 1 } => &[7, 9, 10, 11],
        PortId::Spi { bus: 1, cs: 0 } => &[18, 19, 20, 21],
        PortId::Spi { bus: 1, cs: 1 } => &[17, 19, 20, 21],
        PortId::Spi { bus: 1, cs: 2 } => &[16, 19, 20, 21],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PortId;

    #[test]
    fn forty_pins_in_order_each_gpio_once() {
        for (i, p) in HEADER.iter().enumerate() {
            assert_eq!(usize::from(p.pin), i + 1);
        }
        let mut lines: Vec<u32> = HEADER.iter().filter_map(|p| p.line).collect();
        lines.sort();
        assert_eq!(lines, (0..=27).collect::<Vec<_>>(), "GPIO0 to GPIO27, once each");
        assert!(
            HEADER.iter().filter(|p| p.line.is_some()).all(|p| p.name == format!("GPIO{}", p.line.unwrap()))
        );
        assert_eq!(HEADER.iter().filter(|p| p.name == "GND").count(), 8);
    }

    #[test]
    fn well_known_pins() {
        assert_eq!(pin_of(17).unwrap().pin, 11);
        assert_eq!(pin_of(2).unwrap().function, "I²C 1 SDA");
        assert_eq!(pin_of(14).unwrap().pin, 8);
        assert_eq!(pin_of(28), None);
        assert!(!usable_lines().any(|l| l < 2), "the HAT EEPROM's lines are never offered");
        assert_eq!(usable_lines().count(), 26);
        assert_eq!(bus_lines(&PortId::I2c(1)), [2, 3]);
        assert!(bus_lines(&PortId::Gpio(4)).is_empty());
    }
}
