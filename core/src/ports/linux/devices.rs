//! The Linux bus devices, through `i2cdev`, `spidev` and `gpiocdev`. Linux only.

use crate::ports::buses::{BusPort, BusSupport, Hardware, I2cBus};
use crate::ports::{PortId, header};
use faber_connector::bus::{Bias, BusError, GpioLine, SpiDevice};
use gpiocdev::Request;
use gpiocdev::chip::Chip;
use gpiocdev::line::{self, EdgeDetection, EdgeKind, Value};
use i2cdev::core::{I2CMessage, I2CTransfer};
use i2cdev::linux::{LinuxI2CBus, LinuxI2CMessage};
use spidev::{Spidev, SpidevTransfer};
use std::io;
use std::path::PathBuf;

/// The machine's own buses.
pub struct LinuxHardware;

/// The GPIO chip whose lines are on the header.
fn header_chip() -> Option<Chip> {
    let chips = gpiocdev::chip::chips().ok()?;
    chips.into_iter().find_map(|path: PathBuf| {
        let chip = Chip::from_path(&path).ok()?;
        let info = chip.info().ok()?;
        super::is_header_chip(&info.label).then_some(chip)
    })
}

/// Who uses a line outside Faber (a kernel driver, another program), if anyone.
fn used_by(chip: &Chip, line: u32) -> Option<String> {
    let info = chip.line_info(line).ok()?;
    info.used.then(|| super::line_user(&info.consumer))
}

impl Hardware for LinuxHardware {
    fn support(&self) -> BusSupport {
        BusSupport::System
    }

    fn list(&self) -> Vec<BusPort> {
        let mut out = Vec::new();
        if let Ok(dir) = std::fs::read_dir("/dev") {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = format!("/dev/{name}");
                if let Some(bus) = super::i2c_bus_of(&name) {
                    out.push(BusPort { id: PortId::I2c(bus), path, used_by: None });
                } else if let Some((bus, cs)) = super::spi_device_of(&name) {
                    out.push(BusPort { id: PortId::Spi { bus, cs }, path, used_by: None });
                }
            }
        }
        if let Some(chip) = header_chip() {
            for line in header::usable_lines() {
                let path = format!("{} line {line}", chip.path().display());
                out.push(BusPort { id: PortId::Gpio(line), path, used_by: used_by(&chip, line) });
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    fn open_i2c(&self, bus: u32) -> io::Result<Box<dyn I2cBus>> {
        let bus = LinuxI2CBus::new(format!("/dev/i2c-{bus}")).map_err(io::Error::from)?;
        Ok(Box::new(LinuxI2c(bus)))
    }

    fn open_spi(&self, bus: u32, cs: u32) -> io::Result<Box<dyn SpiDevice>> {
        Ok(Box::new(LinuxSpi(Spidev::open(format!("/dev/spidev{bus}.{cs}"))?)))
    }

    fn open_gpio(&self, line: u32, consumer: &str) -> io::Result<Box<dyn GpioLine>> {
        let chip = header_chip().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "this computer has no Raspberry Pi GPIO header")
        })?;
        if let Some(who) = used_by(&chip, line) {
            return Err(io::Error::new(io::ErrorKind::ResourceBusy, format!("GPIO{line} is used by {who}")));
        }
        Ok(Box::new(LinuxLine {
            chip: chip.path().to_path_buf(),
            line,
            consumer: consumer.to_string(),
            request: None,
        }))
    }
}

struct LinuxI2c(LinuxI2CBus);

impl LinuxI2c {
    fn transfer(&mut self, msgs: &mut [LinuxI2CMessage<'_>]) -> io::Result<()> {
        self.0.transfer(msgs).map(|_| ()).map_err(io::Error::from)
    }
}

impl I2cBus for LinuxI2c {
    fn write_read(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
        let a = u16::from(address);
        self.transfer(&mut [
            LinuxI2CMessage::write(write).with_address(a),
            LinuxI2CMessage::read(read).with_address(a),
        ])
    }

    fn write(&mut self, address: u8, data: &[u8]) -> io::Result<()> {
        self.transfer(&mut [LinuxI2CMessage::write(data).with_address(u16::from(address))])
    }

    fn read(&mut self, address: u8, read: &mut [u8]) -> io::Result<()> {
        self.transfer(&mut [LinuxI2CMessage::read(read).with_address(u16::from(address))])
    }
}

struct LinuxSpi(Spidev);

impl SpiDevice for LinuxSpi {
    fn transfer(&mut self, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        if write.len() != read.len() {
            return Err(BusError::new("an SPI transfer reads as many bytes as it writes"));
        }
        self.0
            .transfer(&mut SpidevTransfer::read_write(write, read))
            .map_err(|e| BusError::new(format!("the SPI transfer failed ({e})")))
    }
}

struct LinuxLine {
    chip: PathBuf,
    line: u32,
    consumer: String,
    /// The kernel's line request, once the line is an input or an output.
    request: Option<Request>,
}

fn gpio_error(line: u32, what: &str, e: impl std::fmt::Display) -> BusError {
    BusError::new(format!("GPIO{line}: {what} failed ({e})"))
}

impl LinuxLine {
    /// Sets the line up; the first time, that requests it from the kernel.
    fn configure(&mut self, set: impl Fn(&mut gpiocdev::request::Config)) -> Result<(), BusError> {
        let mut config = gpiocdev::request::Config::default();
        config.on_chip(&self.chip).with_line(self.line);
        set(&mut config);
        if let Some(request) = &self.request {
            return request.reconfigure(&config).map_err(|e| gpio_error(self.line, "setting it up", e));
        }
        let request = Request::from_config(config)
            .with_consumer(self.consumer.as_str())
            .request()
            .map_err(|e| gpio_error(self.line, "requesting it", e))?;
        self.request = Some(request);
        Ok(())
    }

    fn request(&self) -> Result<&Request, BusError> {
        self.request.as_ref().ok_or_else(|| BusError::new(format!("GPIO{} is not set up yet", self.line)))
    }
}

impl GpioLine for LinuxLine {
    fn line(&self) -> u32 {
        self.line
    }

    fn set_input(&mut self, bias: Bias) -> Result<(), BusError> {
        let bias = match bias {
            Bias::None => line::Bias::Disabled,
            Bias::PullUp => line::Bias::PullUp,
            Bias::PullDown => line::Bias::PullDown,
        };
        self.configure(|c| {
            c.as_input().with_bias(bias).with_edge_detection(EdgeDetection::BothEdges);
        })
    }

    fn set_output(&mut self, high: bool) -> Result<(), BusError> {
        self.configure(|c| {
            c.as_output(Value::from(high));
        })
    }

    fn get(&mut self) -> Result<bool, BusError> {
        let v = self.request()?.value(self.line).map_err(|e| gpio_error(self.line, "reading it", e))?;
        Ok(bool::from(v))
    }

    fn set(&mut self, high: bool) -> Result<(), BusError> {
        self.request()?
            .set_value(self.line, Value::from(high))
            .map_err(|e| gpio_error(self.line, "setting it", e))
    }

    fn edges(&mut self) -> Result<Vec<bool>, BusError> {
        let request = self.request()?;
        let mut out = Vec::new();
        // Only what has arrived: a tick never waits for an edge.
        while request.has_edge_event().map_err(|e| gpio_error(self.line, "reading its edges", e))? {
            let event =
                request.read_edge_event().map_err(|e| gpio_error(self.line, "reading its edges", e))?;
            out.push(event.kind == EdgeKind::Rising);
        }
        Ok(out)
    }
}
