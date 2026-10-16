//! The machine with I²C, SPI and GPIO: ports of every kind, claims per address and per line,
//! GPIO leases and the safe level when a lease ends. The buses are fakes
//! ([`crate::ports::fake`]); nothing here touches real hardware.

use super::*;
use crate::clock::MonotonicClock;
use crate::ports::fake::{FakeHardware, Tmp102};
use crate::ports::{Discovered, FailedUsb, PortKind};
use faber_capabilities::{Command, digital_out};
use std::time::Instant;

struct NoSerial;

impl SerialDiscovery for NoSerial {
    fn list(&self) -> Vec<Discovered> {
        Vec::new()
    }
}

struct NoDisks;

impl DiskDiscovery for NoDisks {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

struct NoUsbProblems;

impl UsbProblemDiscovery for NoUsbProblems {
    fn list(&self) -> Vec<FailedUsb> {
        Vec::new()
    }
}

fn machine(dir: &Path, hardware: &FakeHardware) -> Arc<Machine> {
    Machine::start(options(dir, hardware)).unwrap()
}

/// A connector for an I²C part, at 0x48–0x4b like a TMP102, that only reads nothing: enough to
/// test claims without the declarative engine.
fn i2c_thermometer() -> Definition {
    struct Quiet;
    impl faber_connector::Connector for Quiet {
        fn on_bytes(&mut self, _: &mut dyn faber_connector::Context, _: u64, _: &[u8]) {}
    }
    let manifest = Manifest {
        id: "test-thermometer".into(),
        name: "Test thermometer".into(),
        version: "0.1.0".into(),
        port: PortRequirement::I2c { addresses: vec![0x48, 0x49, 0x4a, 0x4b], default: 0x48 },
        provides: vec![Provide::default_for(Capability::Temperature)],
        settings: Vec::new(),
        usb_hints: Vec::new(),
    };
    Definition::new(manifest, |_| Ok(Box::new(Quiet)))
}

fn add(
    m: &Machine,
    name: &str,
    connector: &str,
    port: &str,
    address: Option<u8>,
) -> Result<PartView, MachineError> {
    m.add_part(NewPart {
        name: name.into(),
        connector: connector.into(),
        port: Some(port.into()),
        address,
        simulate: None,
        mount: Mount::default(),
        settings: Settings::new(),
    })
}

fn wait_until(what: &str, f: impl Fn() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(3), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn running(m: &Machine, name: &str) -> bool {
    m.parts().iter().any(|p| p.name.as_str() == name && p.status == PartStatus::Running)
}

#[test]
fn every_bus_is_a_port_and_a_bus_owns_its_pins() {
    let dir = tempfile::tempdir().unwrap();
    let m = machine(dir.path(), &FakeHardware::standard());
    let ports = m.ports();
    let find =
        |id: &str| ports.iter().find(|p| p.entry.id.to_string() == id).unwrap_or_else(|| panic!("{id}"));
    assert_eq!(find("i2c-1").entry.kind, PortKind::I2c);
    assert_eq!(find("spi-0.1").entry.label, "SPI bus 0, chip select 1");
    assert_eq!(find("gpio-17").entry.label, "GPIO17 (pin 11)");
    assert_eq!(find("gpio-3").entry.reserved.as_deref(), Some("used by I²C bus 1 (i2c-1)"));
    assert_eq!(m.bus_support(), crate::ports::BusSupport::Fake);

    let e = add(&m, "led", crate::connectors::gpio::OUTPUT, "gpio-2", None).unwrap_err();
    assert_eq!(e.code, "wrong-port");
    assert!(e.message.contains("used by I²C bus 1") && e.hint.contains("raspi-config"), "{e:?}");
    m.shutdown();
}

#[test]
fn two_parts_share_a_bus_at_different_addresses_but_never_one_address() {
    let dir = tempfile::tempdir().unwrap();
    let hw = FakeHardware::new().with_i2c(1, 0x48, Tmp102::new(|| 20.0)).with_empty_i2c(0);
    let mut connectors = crate::connectors::built_in();
    connectors.push(i2c_thermometer());
    let m = Machine::start(MachineOptions { connectors, ..options(dir.path(), &hw) }).unwrap();

    let first = add(&m, "inside", "test-thermometer", "i2c-1", None).unwrap();
    assert_eq!(first.address, Some(0x48), "the connector's default address");
    add(&m, "outside", "test-thermometer", "i2c-1", Some(0x49)).unwrap();
    let e = add(&m, "third", "test-thermometer", "i2c-1", Some(0x48)).unwrap_err();
    assert_eq!(e.code, "port-claimed");
    assert!(e.message.contains("Address 0x48 on I²C bus 1 is used by the part \"inside\""), "{e:?}");
    assert!(e.hint.contains("different addresses"), "{e:?}");
    add(&m, "other_bus", "test-thermometer", "i2c-0", Some(0x48)).unwrap();

    let e = add(&m, "far", "test-thermometer", "i2c-1", Some(0x20)).unwrap_err();
    assert_eq!(e.code, "unknown-address");
    assert!(e.hint.contains("0x48, 0x49, 0x4a or 0x4b"), "{e:?}");
    let e = add(&m, "led", crate::connectors::gpio::OUTPUT, "i2c-1", None).unwrap_err();
    assert_eq!(e.code, "wrong-port");
    assert!(e.message.contains("needs a GPIO line"), "{e:?}");

    let bus = m.ports().into_iter().find(|p| p.entry.id == PortId::I2c(1)).unwrap();
    assert_eq!(bus.claimed_by, None, "nobody holds the whole bus");
    assert_eq!(bus.addresses, [(0x48, "inside".to_string()), (0x49, "outside".to_string())]);
    let toml = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
    assert!(toml.contains("port = \"i2c-1\"\naddress = 0x49"), "{toml}");

    m.remove_part("inside").unwrap();
    add(&m, "third", "test-thermometer", "i2c-1", Some(0x48)).unwrap();
    m.shutdown();
}

fn options(dir: &Path, hardware: &FakeHardware) -> MachineOptions {
    MachineOptions {
        data_dir: dir.to_path_buf(),
        clock: Arc::new(MonotonicClock::new()),
        discovery: Arc::new(NoSerial),
        usb_problems: Arc::new(NoUsbProblems),
        disks: Arc::new(NoDisks),
        opener: Arc::new(crate::ports::SystemOpener::new(ReplayLibrary::default())),
        hardware: Arc::new(hardware.clone()),
        replays: ReplayLibrary::default(),
        connectors: crate::connectors::built_in(),
        timing: Timing {
            tick: Duration::from_millis(5),
            backoff_start: Duration::from_millis(10),
            port_retry_max: Duration::from_millis(50),
            ..Timing::default()
        },
        port_scan: None,
        world: Arc::new(simparts::SimWorld::new()),
    }
}

#[test]
fn a_gpio_line_has_one_part_and_the_os_sees_who() {
    let dir = tempfile::tempdir().unwrap();
    let hw = FakeHardware::new().with_gpio_header();
    let m = machine(dir.path(), &hw);
    add(&m, "button", crate::connectors::gpio::INPUT, "gpio-17", None).unwrap();
    wait_until("the button to run", || running(&m, "button"));
    assert_eq!(hw.owner(17).as_deref(), Some("faber:button"));
    let e = add(&m, "led", crate::connectors::gpio::OUTPUT, "gpio-17", None).unwrap_err();
    assert_eq!(e.code, "port-claimed");
    let e = add(&m, "led", crate::connectors::gpio::OUTPUT, "gpio-22", Some(0x48)).unwrap_err();
    assert_eq!(e.code, "address-not-used");

    hw.drive(17, Some(false));
    wait_until("the press to arrive", || {
        let latest = m.router().latest(&SignalAddress::parse("button/digital-in").unwrap()).flatten();
        matches!(latest.as_deref(), Some(Reading::DigitalIn(r)) if r.level)
    });
    m.remove_part("button").unwrap();
    wait_until("the line to be free", || hw.owner(17).is_none());
    m.shutdown();
}

#[test]
fn an_output_needs_a_lease_and_goes_to_its_safe_level_when_the_lease_ends() {
    let dir = tempfile::tempdir().unwrap();
    let hw = FakeHardware::new().with_gpio_header();
    let m = machine(dir.path(), &hw);
    add(&m, "relay", crate::connectors::gpio::OUTPUT, "gpio-22", None).unwrap();
    wait_until("the relay to run", || running(&m, "relay"));
    assert_eq!((hw.is_output(22), hw.level(22)), (true, Some(false)), "it starts inactive");

    let on = Command::DigitalOut(digital_out::Command::Set { level: true });
    assert_eq!(m.command("relay", "no-lease", on).unwrap_err().code, "lease-required");
    let lease = m.request_lease("relay", "test", false).unwrap();
    m.command("relay", &lease.id, on).unwrap();
    assert_eq!(hw.level(22), Some(true));
    let sent = Instant::now();
    wait_until("the lease to end", || hw.level(22) == Some(false));
    assert!(sent.elapsed() >= Duration::from_millis(900), "not before its 1 s ran out");
    m.shutdown();
}

#[test]
fn a_scan_finds_what_answers_suggests_what_it_might_be_and_leaves_parts_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut connectors = crate::connectors::built_in();
    connectors.push(i2c_thermometer());
    let m = Machine::start(MachineOptions { connectors, ..options(dir.path(), &FakeHardware::standard()) })
        .unwrap();
    add(&m, "inside", "test-thermometer", "i2c-1", Some(0x4a)).unwrap();
    let scan = m.scan_i2c("i2c-1").unwrap();
    let found: Vec<(u8, Option<&str>)> =
        scan.found.iter().map(|f| (f.address, f.used_by.as_deref())).collect();
    assert_eq!(found, [(0x48, None), (0x4a, Some("inside")), (0x76, None)]);
    let tmp = &scan.found[0].suggestions;
    assert!(tmp.iter().any(|s| s.chip.contains("TMP102")), "{tmp:?}");
    assert!(tmp.iter().any(|s| s.connector.as_deref() == Some("test-thermometer")), "{tmp:?}");
    assert!(scan.found[2].suggestions[0].chip.contains("BME280"));

    assert_eq!(m.scan_i2c("gpio-17").unwrap_err().code, "not-an-i2c-bus");
    assert_eq!(m.scan_i2c("i2c-7").unwrap_err().code, "unknown-port");
    m.shutdown();
}
