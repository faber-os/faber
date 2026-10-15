//! The `faber-bridge` connector's manifest: a microcontroller running FaberLink on a serial port
//! (docs/faberlink.md). Its parts (motors, sensors) are not in the manifest: the board says which
//! it has, and each becomes a sub-part (`mega.left`). The machine runs a bridge itself
//! (`crate::bridge`), not through the runtime, because it writes to its port, hosts sub-parts
//! and carries the e-stop; this definition is what Add part offers and what checks its settings.

use faber_capabilities::{Info, estop};
use faber_connector::settings::Reader;
use faber_connector::{
    Connector, Context, Definition, Manifest, PortRequirement, Provide, Settings, SettingsError,
};

/// Also `config::BRIDGE_CONNECTOR`, which parts.toml checks nested parts against.
pub const ID: &str = "faber-bridge";

/// FaberLink's speed (docs/faberlink.md §2): works on every Arduino.
pub const BAUD: u32 = 115_200;

/// The bridge's own signal: the board's stop button, `<bridge>/estop`.
pub const ESTOP_SIGNAL: &str = "estop";

pub fn manifest() -> Manifest {
    Manifest {
        id: ID.into(),
        name: "FaberLink board".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PortRequirement::Serial { baud: BAUD },
        provides: vec![Provide {
            name: ESTOP_SIGNAL.into(),
            ..Provide::with_info(Info::Estop(estop::Info { hardware_latching: true }))
        }],
        settings: Vec::new(),
        usb_hints: Vec::new(),
    }
}

pub fn definition() -> Definition {
    Definition::new(manifest(), create)
}

/// Only checks the settings (there are none): the machine runs bridges itself.
fn create(settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    Reader::new(settings, &[], "a FaberLink board")?;
    Ok(Box::new(RunByTheMachine))
}

/// Stands in for a bridge where a connector is expected; never fed bytes.
struct RunByTheMachine;

impl Connector for RunByTheMachine {
    fn on_bytes(&mut self, _: &mut dyn Context, _: u64, _: &[u8]) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::Capability;

    #[test]
    fn a_bridge_is_a_serial_part_with_the_boards_stop_button() {
        let m = manifest();
        assert_eq!(m.validate(), Ok(()));
        assert_eq!(m.port, PortRequirement::Serial { baud: 115_200 });
        assert_eq!(m.provides[0].name, "estop");
        assert_eq!(m.provides[0].capability, Capability::Estop);
    }

    #[test]
    fn a_bridge_takes_no_settings() {
        assert!((definition().create)(&Settings::new()).is_ok());
        let mut s = Settings::new();
        s.insert("baud".into(), 9600.into());
        let e = (definition().create)(&s).err().unwrap();
        assert_eq!(e.key, "baud");
        assert!(e.hint.contains("needs no settings"), "{e:?}");
    }
}
