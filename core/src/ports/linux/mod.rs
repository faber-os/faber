//! The real buses on Linux: I²C through `/dev/i2c-*` (the `I2C_RDWR` ioctl, so a register read
//! is one transaction with a repeated start), SPI through `/dev/spidev*`, and GPIO through the
//! character device's **v2** interface (`/dev/gpiochip*`), never the deprecated sysfs one.
//!
//! The ioctls come from small, maintained crates (`i2cdev` and `spidev` from the Rust Embedded
//! working group, `gpiocdev` for the v2 GPIO interface) rather than our own: the workspace
//! forbids `unsafe` code, and an ioctl can't be called without it. The I/O only compiles on
//! Linux; the naming rules below are pure and tested everywhere.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

#[cfg(target_os = "linux")]
mod devices;

#[cfg(target_os = "linux")]
pub use devices::LinuxHardware;

/// `i2c-1` (a file in `/dev`) → bus 1.
pub fn i2c_bus_of(file_name: &str) -> Option<u32> {
    file_name.strip_prefix("i2c-")?.parse().ok()
}

/// `spidev0.1` (a file in `/dev`) → bus 0, chip select 1.
pub fn spi_device_of(file_name: &str) -> Option<(u32, u32)> {
    let (bus, cs) = file_name.strip_prefix("spidev")?.split_once('.')?;
    Some((bus.parse().ok()?, cs.parse().ok()?))
}

/// The GPIO chip whose lines come out on the 40-pin header, by its label: the Pi 5's RP1, or the
/// SoC's own on a Pi 4, 3, 2, Zero and 1.
pub fn is_header_chip(label: &str) -> bool {
    matches!(label, "pinctrl-rp1" | "pinctrl-bcm2711" | "pinctrl-bcm2835")
}

/// What the OS shows as a line's user while a part has it: `faber:<part>`, cut to the kernel's
/// 31 bytes on a character boundary.
pub fn consumer(part: &str) -> String {
    let mut s = format!("faber:{part}");
    let mut end = s.len().min(31);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s
}

/// Who uses a line, as the OS names them; the kernel leaves drivers' names empty sometimes.
pub fn line_user(consumer: &str) -> String {
    if consumer.is_empty() { "the system".into() } else { consumer.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_files_name_their_bus() {
        assert_eq!(i2c_bus_of("i2c-1"), Some(1));
        assert_eq!(i2c_bus_of("i2c-20"), Some(20));
        assert_eq!(i2c_bus_of("i2c-dev"), None);
        assert_eq!(spi_device_of("spidev0.1"), Some((0, 1)));
        assert_eq!(spi_device_of("spidev10"), None);
        assert_eq!(spi_device_of("spidevx.0"), None);
    }

    #[test]
    fn the_header_chip_is_found_by_its_label() {
        for label in ["pinctrl-rp1", "pinctrl-bcm2711", "pinctrl-bcm2835"] {
            assert!(is_header_chip(label), "{label}");
        }
        assert!(!is_header_chip("rp1-gpio") && !is_header_chip("gpio-brcmstb@107d508500"));
    }

    #[test]
    fn consumers_fit_the_kernels_name_field() {
        assert_eq!(consumer("button"), "faber:button");
        let long = consumer(&"é".repeat(20));
        assert!(long.len() <= 31 && long.starts_with("faber:é"), "{long}");
        assert_eq!(line_user(""), "the system");
        assert_eq!(line_user("spi0 CS0"), "spi0 CS0");
    }
}
