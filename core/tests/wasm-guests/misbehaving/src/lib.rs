//! A connector that misbehaves on purpose, one way per byte it receives, to test the sandbox in
//! `core/tests/wasm.rs`. Written like any built-in connector; built as a component there.

use faber_capabilities::{Command, Reading, distance, motor};
use faber_connector::{
    Applied, CommandError, Connector, Context, Definition, Health, Manifest, PortRequirement, Provide,
    Settings, SettingsError,
};

pub const ID: &str = "test-guest";

/// What each byte makes it do.
pub mod byte {
    /// Publish a distance reading and report Running.
    pub const PUBLISH: u8 = 1;
    /// Never return.
    pub const LOOP_FOREVER: u8 = 2;
    /// Allocate 64 MB.
    pub const HOG_MEMORY: u8 = 3;
    /// Panic.
    pub const PANIC: u8 = 4;
    /// Publish to a signal its manifest doesn't declare.
    pub const UNDECLARED: u8 = 5;
}

pub fn manifest() -> Manifest {
    Manifest {
        id: ID.into(),
        name: "Test guest".into(),
        version: "0.1.0".into(),
        port: PortRequirement::Serial { baud: 9600 },
        provides: vec![Provide::default_for(faber_capabilities::Capability::Distance)],
        settings: Vec::new(),
        usb_hints: Vec::new(),
    }
}

pub fn definition() -> Definition {
    Definition::new(manifest(), create)
}

fn create(settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    faber_connector::settings::Reader::new(settings, &manifest().settings, "the test guest")?;
    Ok(Box::new(Guest))
}

#[cfg(target_arch = "wasm32")]
faber_connector_wasm::export!(definition);

struct Guest;

fn reading(t_us: u64) -> Reading {
    Reading::Distance(distance::Reading { t_us, meters: 1.5, status: distance::Status::Ok })
}

impl Connector for Guest {
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: u64, bytes: &[u8]) {
        for &b in bytes {
            match b {
                byte::PUBLISH => {
                    ctx.set_health(Health::Running);
                    ctx.publish("distance", reading(t_us));
                }
                byte::LOOP_FOREVER => {
                    let mut n = 0u64;
                    loop {
                        n = std::hint::black_box(n.wrapping_add(1));
                    }
                }
                byte::HOG_MEMORY => {
                    let hog = vec![1u8; 64 << 20];
                    std::hint::black_box(&hog);
                }
                byte::PANIC => panic!("bad byte"),
                byte::UNDECLARED => ctx.publish("elsewhere", reading(t_us)),
                _ => {}
            }
        }
    }

    /// `set-duty` is applied at half the duty; anything else is refused.
    fn command(&mut self, _: &mut dyn Context, command: Command) -> Result<Applied, CommandError> {
        match command {
            Command::Motor(motor::Command::SetDuty { duty }) => {
                Ok(Applied { command: Command::Motor(motor::Command::SetDuty { duty: duty / 2.0 }) })
            }
            _ => Err(CommandError::Unsupported),
        }
    }
}
