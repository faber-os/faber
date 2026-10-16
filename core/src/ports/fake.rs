//! Fake buses with fake parts on them, for tests and for trying Faber without a Pi
//! (`faber-core --fake-hardware`). They behave like the real ones as far as Faber can tell: an
//! I²C bus whose devices answer or don't, a GPIO chip whose lines are inputs or outputs, an SPI
//! device with MOSI wired to MISO.

use super::buses::{BusPort, BusSupport, Hardware, I2cBus};
use super::{PortId, header};
use faber_connector::bus::{Bias, BusError, GpioLine, SpiDevice};
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

/// A simulated I²C device: what it does with the bytes a master writes, and what it answers.
pub trait FakeI2cDevice: Send {
    fn write(&mut self, data: &[u8]);
    fn read(&mut self, buf: &mut [u8]);
}

/// A TI TMP102 temperature sensor, after its datasheet (SBOS397, §7.5): a pointer register picks
/// one of four 16-bit registers, sent most significant byte first.
///
/// - `0x00` temperature, read-only: 12 bits, left-justified, 0.0625 °C per bit, two's complement
///   (13 bits, with bit 0 set, in extended mode).
/// - `0x01` configuration, power-on `0x60A0`: 12-bit resolution, 4 Hz conversions. Bit 4 of the
///   second byte (EM) switches extended mode on.
/// - `0x02` T(LOW), power-on 75 °C; `0x03` T(HIGH), power-on 80 °C.
pub struct Tmp102 {
    pointer: u8,
    config: u16,
    limits: [u16; 2],
    celsius: Box<dyn Fn() -> f32 + Send>,
}

impl Tmp102 {
    pub const CONFIG_POWER_ON: u16 = 0x60a0;
    const EXTENDED: u16 = 0x0010;

    /// One that reads `celsius()` whenever it is asked.
    pub fn new(celsius: impl Fn() -> f32 + Send + 'static) -> Tmp102 {
        Tmp102 {
            pointer: 0,
            config: Tmp102::CONFIG_POWER_ON,
            limits: [0x4b00, 0x5000],
            celsius: Box::new(celsius),
        }
    }

    /// The temperature register's value for `celsius`, as the sensor would send it.
    pub fn encode(celsius: f32, extended: bool) -> u16 {
        let (bits, max) = if extended { (13, 150.0) } else { (12, 127.9375) };
        let counts = (celsius.clamp(-55.0, max) / 0.0625).round() as i16;
        if extended { ((counts << 3) as u16) | 1 } else { (counts << (16 - bits)) as u16 }
    }

    fn register(&self) -> u16 {
        match self.pointer & 0b11 {
            0 => Tmp102::encode((self.celsius)(), self.config & Tmp102::EXTENDED != 0),
            1 => self.config,
            r => self.limits[usize::from(r - 2)],
        }
    }
}

impl FakeI2cDevice for Tmp102 {
    fn write(&mut self, data: &[u8]) {
        let Some((&pointer, value)) = data.split_first() else { return };
        self.pointer = pointer & 0b11;
        if let [hi, lo, ..] = *value {
            let v = u16::from_be_bytes([hi, lo]);
            match self.pointer {
                // The first byte's top bits (OS, R1, R0) are read-only.
                1 => self.config = (v & 0x1ff0) | (Tmp102::CONFIG_POWER_ON & 0x6000),
                2 | 3 => self.limits[usize::from(self.pointer - 2)] = v,
                _ => {} // the temperature register is read-only
            }
        }
    }

    fn read(&mut self, buf: &mut [u8]) {
        let bytes = self.register().to_be_bytes();
        for (i, b) in buf.iter_mut().enumerate() {
            *b = bytes[i % 2];
        }
    }
}

/// A device that is only a map of byte registers with an auto-incrementing pointer, the way
/// many sensors work (e.g. a BME280 that says who it is: `0xD0` reads `0x60`).
pub struct Registers {
    pointer: u8,
    values: BTreeMap<u8, u8>,
}

impl Registers {
    pub fn new(values: impl IntoIterator<Item = (u8, u8)>) -> Registers {
        Registers { pointer: 0, values: values.into_iter().collect() }
    }
}

impl FakeI2cDevice for Registers {
    fn write(&mut self, data: &[u8]) {
        let Some((&pointer, value)) = data.split_first() else { return };
        self.pointer = pointer;
        for &v in value {
            self.values.insert(self.pointer, v);
            self.pointer = self.pointer.wrapping_add(1);
        }
    }

    fn read(&mut self, buf: &mut [u8]) {
        for b in buf {
            *b = self.values.get(&self.pointer).copied().unwrap_or(0);
            self.pointer = self.pointer.wrapping_add(1);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Not set up by anyone yet: an input with no pull.
    Unset,
    Input(Bias),
    Output(bool),
}

struct Line {
    owner: Option<String>,
    mode: Mode,
    /// The level something outside drives onto an input (a button), if anything does.
    driven: Option<bool>,
    /// Levels an input changed to that its owner hasn't collected yet.
    edges: Vec<bool>,
}

impl Line {
    fn level(&self) -> bool {
        match self.mode {
            Mode::Output(high) => high,
            Mode::Input(bias) => self.driven.unwrap_or(bias == Bias::PullUp),
            Mode::Unset => self.driven.unwrap_or(false),
        }
    }
}

#[derive(Default)]
struct State {
    i2c: BTreeMap<u32, BTreeMap<u8, Box<dyn FakeI2cDevice>>>,
    spi: BTreeSet<(u32, u32)>,
    gpio: BTreeMap<u32, Line>,
}

/// Fake buses. Clones share them, so a test keeps one to look at what parts did.
#[derive(Clone, Default)]
pub struct FakeHardware {
    state: Arc<Mutex<State>>,
}

impl FakeHardware {
    /// No buses at all, until added.
    pub fn new() -> FakeHardware {
        FakeHardware::default()
    }

    /// What `--fake-hardware` attaches: I²C bus 1 with a TMP102 at 0x48 (about 21.5 °C, drifting
    /// slowly so a plot shows something) and a BME280 at 0x76 that only says who it is, SPI 0 with
    /// both chip selects looped back, and every GPIO on the header.
    pub fn standard() -> FakeHardware {
        let start = std::time::Instant::now();
        let drift = move || 21.5 + 1.5 * (start.elapsed().as_secs_f32() * std::f32::consts::TAU / 60.0).sin();
        FakeHardware::new()
            .with_i2c(1, 0x48, Tmp102::new(drift))
            .with_i2c(1, 0x76, Registers::new([(0xd0, 0x60)]))
            .with_spi(0, 0)
            .with_spi(0, 1)
            .with_gpio_header()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("fake hardware lock")
    }

    /// Puts `device` on I²C bus `bus` at `address` (and makes the bus if it is new).
    pub fn with_i2c(self, bus: u32, address: u8, device: impl FakeI2cDevice + 'static) -> FakeHardware {
        self.lock().i2c.entry(bus).or_default().insert(address, Box::new(device));
        self
    }

    /// An I²C bus with nothing on it.
    pub fn with_empty_i2c(self, bus: u32) -> FakeHardware {
        self.lock().i2c.entry(bus).or_default();
        self
    }

    pub fn with_spi(self, bus: u32, cs: u32) -> FakeHardware {
        self.lock().spi.insert((bus, cs));
        self
    }

    /// Every GPIO line Faber offers on the header, all unused inputs.
    pub fn with_gpio_header(self) -> FakeHardware {
        let mut st = self.lock();
        for line in header::usable_lines() {
            st.gpio.insert(line, Line { owner: None, mode: Mode::Unset, driven: None, edges: Vec::new() });
        }
        drop(st);
        self
    }

    /// Drives an input from outside, like a button: `Some(level)`, or `None` to let it float
    /// (then its pull resistor decides).
    pub fn drive(&self, line: u32, level: Option<bool>) {
        let mut st = self.lock();
        let Some(l) = st.gpio.get_mut(&line) else { return };
        let before = l.level();
        l.driven = level;
        if l.owner.is_some() && matches!(l.mode, Mode::Input(_)) && l.level() != before {
            let now = l.level();
            l.edges.push(now);
        }
    }

    /// The level on a line's pin now.
    pub fn level(&self, line: u32) -> Option<bool> {
        self.lock().gpio.get(&line).map(Line::level)
    }

    /// True while a line is an output.
    pub fn is_output(&self, line: u32) -> bool {
        self.lock().gpio.get(&line).is_some_and(|l| matches!(l.mode, Mode::Output(_)))
    }

    /// Who has the line open (`faber:<part>`), as the OS would show it.
    pub fn owner(&self, line: u32) -> Option<String> {
        self.lock().gpio.get(&line).and_then(|l| l.owner.clone())
    }
}

impl Hardware for FakeHardware {
    fn support(&self) -> BusSupport {
        BusSupport::Fake
    }

    fn list(&self) -> Vec<BusPort> {
        let st = self.lock();
        let i2c = st.i2c.keys().map(|&bus| BusPort {
            id: PortId::I2c(bus),
            path: format!("/dev/i2c-{bus} (simulated)"),
            used_by: None,
        });
        let spi = st.spi.iter().map(|&(bus, cs)| BusPort {
            id: PortId::Spi { bus, cs },
            path: format!("/dev/spidev{bus}.{cs} (simulated)"),
            used_by: None,
        });
        let gpio = st.gpio.keys().map(|&line| BusPort {
            id: PortId::Gpio(line),
            path: format!("GPIO{line} (simulated)"),
            used_by: None,
        });
        i2c.chain(spi).chain(gpio).collect()
    }

    fn open_i2c(&self, bus: u32) -> io::Result<Box<dyn I2cBus>> {
        if !self.lock().i2c.contains_key(&bus) {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("there is no I²C bus {bus}")));
        }
        Ok(Box::new(FakeBus { hardware: self.clone(), bus }))
    }

    fn open_spi(&self, bus: u32, cs: u32) -> io::Result<Box<dyn SpiDevice>> {
        if !self.lock().spi.contains(&(bus, cs)) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("there is no SPI device {bus}.{cs}"),
            ));
        }
        Ok(Box::new(Loopback))
    }

    fn open_gpio(&self, line: u32, consumer: &str) -> io::Result<Box<dyn GpioLine>> {
        let mut st = self.lock();
        let l = st
            .gpio
            .get_mut(&line)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("there is no GPIO{line}")))?;
        if let Some(owner) = &l.owner {
            return Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                format!("GPIO{line} is used by {owner}"),
            ));
        }
        l.owner = Some(consumer.to_string());
        l.edges.clear();
        Ok(Box::new(FakeLine { hardware: self.clone(), line }))
    }
}

struct FakeBus {
    hardware: FakeHardware,
    bus: u32,
}

impl FakeBus {
    /// Runs `f` on the device at `address`, or fails as a real bus does when nothing answers.
    fn with(&self, address: u8, f: impl FnOnce(&mut dyn FakeI2cDevice)) -> io::Result<()> {
        let mut st = self.hardware.lock();
        let device = st.i2c.get_mut(&self.bus).and_then(|b| b.get_mut(&address));
        match device {
            Some(d) => {
                f(d.as_mut());
                Ok(())
            }
            // What Linux says when no device acknowledges its address (ENXIO / EREMOTEIO).
            None => Err(io::Error::other(format!("nothing acknowledged address 0x{address:02x}"))),
        }
    }
}

impl I2cBus for FakeBus {
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
        self.with(address, |d| {
            d.write(write);
            d.read(read);
        })
    }

    fn write(&mut self, address: u8, data: &[u8]) -> io::Result<()> {
        self.with(address, |d| d.write(data))
    }

    fn read(&mut self, address: u8, read: &mut [u8]) -> io::Result<()> {
        self.with(address, |d| d.read(read))
    }
}

/// MOSI wired to MISO: every byte clocked out comes straight back.
struct Loopback;

impl SpiDevice for Loopback {
    fn transfer(&mut self, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        if write.len() != read.len() {
            return Err(BusError::new("an SPI transfer reads as many bytes as it writes"));
        }
        read.copy_from_slice(write);
        Ok(())
    }
}

struct FakeLine {
    hardware: FakeHardware,
    line: u32,
}

impl FakeLine {
    fn with<T>(&self, f: impl FnOnce(&mut Line) -> T) -> T {
        let mut st = self.hardware.lock();
        f(st.gpio.get_mut(&self.line).expect("an open line exists"))
    }
}

impl GpioLine for FakeLine {
    fn line(&self) -> u32 {
        self.line
    }

    fn set_input(&mut self, bias: Bias) -> Result<(), BusError> {
        self.with(|l| {
            l.mode = Mode::Input(bias);
            l.edges.clear();
        });
        Ok(())
    }

    fn set_output(&mut self, high: bool) -> Result<(), BusError> {
        self.with(|l| {
            l.mode = Mode::Output(high);
            l.edges.clear();
        });
        Ok(())
    }

    fn get(&mut self) -> Result<bool, BusError> {
        Ok(self.with(|l| l.level()))
    }

    fn set(&mut self, high: bool) -> Result<(), BusError> {
        self.with(|l| match l.mode {
            Mode::Output(_) => {
                l.mode = Mode::Output(high);
                Ok(())
            }
            _ => Err(BusError::new(format!("GPIO{} is not an output", self.line))),
        })
    }

    fn edges(&mut self) -> Result<Vec<bool>, BusError> {
        Ok(self.with(|l| std::mem::take(&mut l.edges)))
    }
}

impl Drop for FakeLine {
    /// Like the kernel: the line is free again, and an output keeps its level.
    fn drop(&mut self) {
        self.with(|l| l.owner = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp102_encodes_as_its_datasheet_table_says() {
        // SBOS397 table 8-1 (12-bit) and 8-2 (13-bit, extended mode).
        for (c, raw) in [(127.9375, 0x7ff0), (100.0, 0x6400), (25.0, 0x1900), (0.0625, 0x0010), (0.0, 0x0000)]
        {
            assert_eq!(Tmp102::encode(c, false), raw, "{c} °C");
        }
        for (c, raw) in [(-0.0625, 0xfff0), (-25.0, 0xe700), (-55.0, 0xc900)] {
            assert_eq!(Tmp102::encode(c, false), raw, "{c} °C");
        }
        assert_eq!(Tmp102::encode(150.0, true), 0x4b01, "13 bits, EM flag in bit 0");
        assert_eq!(Tmp102::encode(25.0, true), 0x0c81);
    }

    #[test]
    fn tmp102_registers_through_its_pointer() {
        let celsius = Arc::new(Mutex::new(21.5_f32));
        let knob = Arc::clone(&celsius);
        let hw = FakeHardware::new().with_i2c(1, 0x48, Tmp102::new(move || *knob.lock().unwrap()));
        let mut bus = hw.open_i2c(1).unwrap();
        let mut buf = [0; 2];
        bus.write_read(0x48, &[0x00], &mut buf).unwrap();
        assert_eq!(buf, [0x15, 0x80], "21.5 °C = 344 counts, left-justified");
        *celsius.lock().unwrap() = -10.25;
        bus.read(0x48, &mut buf).unwrap();
        assert_eq!(i16::from_be_bytes(buf) >> 4, -164, "the pointer stays on the temperature");
        bus.write_read(0x48, &[0x01], &mut buf).unwrap();
        assert_eq!(buf, [0x60, 0xa0], "power-on configuration");
        bus.write(0x48, &[0x01, 0x60, 0xb0]).unwrap();
        bus.write_read(0x48, &[0x00], &mut buf).unwrap();
        assert_eq!(buf[1] & 1, 1, "extended mode sets bit 0");
        bus.write(0x48, &[0x00, 0x12, 0x34]).unwrap();
        bus.write_read(0x48, &[0x02], &mut buf).unwrap();
        assert_eq!(buf, [0x4b, 0x00], "T(LOW) is 75 °C at power-on");
        let e = bus.read(0x49, &mut buf).unwrap_err();
        assert!(e.to_string().contains("0x49"), "{e}");
        assert!(hw.open_i2c(0).is_err());
    }

    #[test]
    fn gpio_lines_are_inputs_or_outputs_and_keep_their_edges() {
        let hw = FakeHardware::new().with_gpio_header();
        let mut button = hw.open_gpio(17, "faber:button").unwrap();
        assert_eq!(hw.owner(17).as_deref(), Some("faber:button"));
        assert!(hw.open_gpio(17, "faber:other").err().unwrap().to_string().contains("used by faber:button"));
        button.set_input(Bias::PullUp).unwrap();
        assert!(button.get().unwrap(), "pulled up");
        hw.drive(17, Some(false));
        hw.drive(17, Some(true));
        hw.drive(17, Some(false));
        assert_eq!(button.edges().unwrap(), [false, true, false], "no edge between two ticks is lost");
        assert!(button.edges().unwrap().is_empty());
        assert!(button.set(true).is_err(), "an input can't be driven");
        drop(button);
        assert_eq!(hw.owner(17), None);

        let mut relay = hw.open_gpio(22, "faber:relay").unwrap();
        relay.set_output(false).unwrap();
        relay.set(true).unwrap();
        assert_eq!(hw.level(22), Some(true));
        assert!(hw.is_output(22));
        drop(relay);
        assert_eq!(hw.level(22), Some(true), "a released output keeps its level, as on Linux");
        assert!(hw.open_gpio(0, "x").is_err(), "GPIO0 belongs to the HAT EEPROM, not on the list");
    }

    #[test]
    fn spi_loops_back_and_the_standard_set_has_every_kind() {
        let hw = FakeHardware::standard();
        let mut spi = hw.open_spi(0, 1).unwrap();
        let mut back = [0; 3];
        spi.transfer(&[1, 2, 3], &mut back).unwrap();
        assert_eq!(back, [1, 2, 3]);
        assert!(spi.transfer(&[1], &mut back).is_err());
        let ids: Vec<String> = hw.list().iter().map(|p| p.id.to_string()).collect();
        assert!(ids.starts_with(&["i2c-1".to_string(), "spi-0.0".into(), "spi-0.1".into()]), "{ids:?}");
        assert!(ids.contains(&"gpio-17".to_string()));
        assert_eq!(hw.support(), BusSupport::Fake);
        let mut bus = hw.open_i2c(1).unwrap();
        let mut id = [0];
        bus.write_read(0x76, &[0xd0], &mut id).unwrap();
        assert_eq!(id, [0x60], "a BME280 says who it is");
        let mut t = [0; 2];
        bus.write_read(0x48, &[0], &mut t).unwrap();
        let c = f32::from(i16::from_be_bytes(t) >> 4) * 0.0625;
        assert!((19.9..=23.1).contains(&c), "{c}");
    }
}
