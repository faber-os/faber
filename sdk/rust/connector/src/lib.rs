//! The Faber connector interface.
//!
//! A connector turns one kind of hardware into capabilities. It never does I/O itself: the core
//! opens the port and hands the connector **bytes in**; the connector hands back **readings out**
//! through a [`Context`]. That keeps connectors pure, testable without hardware, and ready to be
//! compiled to WebAssembly (Phase 2) with the same shape.
//!
//! See docs/architecture.md §4 and docs/development.md.

pub mod capture;
pub mod testing;

use faber_capabilities::{Capability, Reading, Timestamp};

/// Settings for one part, straight from its `[parts.<name>.settings]` table.
pub type Settings = toml::Table;

/// What a connector is and what it needs. Shown in the shell and checked before a part starts.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// Package id, e.g. `ldrobot-ld19`.
    pub id: String,
    pub name: String,
    pub version: String,
    pub port: PortRequirement,
    pub provides: Vec<Provide>,
    /// USB adapters this hardware often comes with, for detect-then-suggest (docs/principles.md).
    pub usb_hints: Vec<UsbHint>,
}

/// "When this USB adapter shows up, it might be our hardware", and why, in plain words. Be honest:
/// a generic chip gets "often comes with", never "this is".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbHint {
    pub vid: u16,
    pub pid: u16,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PortRequirement {
    /// Any serial port (UART or USB serial) at this baud rate.
    Serial { baud: u32 },
}

/// One capability instance a part offers. Its signal address is `<part>/<name>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Provide {
    pub name: String,
    pub capability: Capability,
}

impl Provide {
    /// Uses the capability's own name, the default (docs/capabilities.md, rule 8).
    pub fn default_for(capability: Capability) -> Provide {
        Provide { name: capability.name().to_string(), capability }
    }
}

/// A connector's own view of how it's doing. The core turns it into the part's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Health {
    /// Waiting for the first good data.
    Starting,
    Running,
    /// Working, but something is off.
    Degraded(Problem),
    /// Not working.
    Failed(Problem),
}

/// What is wrong and what the user should try, in plain words (docs/principles.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub message: String,
    pub hint: String,
}

impl Problem {
    pub fn new(message: impl Into<String>, hint: impl Into<String>) -> Problem {
        Problem { message: message.into(), hint: hint.into() }
    }
}

/// What a connector can do while it runs. Implemented by the core (and by
/// [`testing::RecordingContext`] in tests).
pub trait Context {
    /// Publishes a reading on one of the manifest's `provides` names.
    fn publish(&mut self, name: &str, reading: Reading);
    fn set_health(&mut self, health: Health);
}

/// The behaviour of a running part.
pub trait Connector: Send {
    /// Bytes arrived from the part's port. `t_us` is when the core read them.
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]);

    /// Called regularly (about every 100 ms) even when no bytes arrive, so a connector can notice
    /// silence. `now_us` is the core's current time.
    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        let _ = (ctx, now_us);
    }
}

/// A setting that can't be used, with a message and a hint that say how to fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    pub key: String,
    pub message: String,
    pub hint: String,
}

/// Everything the core needs to offer and start a connector.
pub struct Definition {
    pub manifest: Manifest,
    pub create: fn(&Settings) -> Result<Box<dyn Connector>, SettingsError>,
}

/// Why a manifest is not usable.
#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    BadId(String),
    BadProvideName(String),
    DuplicateProvideName(String),
    NothingProvided,
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if !is_kebab_name(&self.id) {
            return Err(ManifestError::BadId(self.id.clone()));
        }
        if self.provides.is_empty() {
            return Err(ManifestError::NothingProvided);
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.provides {
            if !is_kebab_name(&p.name) {
                return Err(ManifestError::BadProvideName(p.name.clone()));
            }
            if !seen.insert(p.name.as_str()) {
                return Err(ManifestError::DuplicateProvideName(p.name.clone()));
            }
        }
        Ok(())
    }

    pub fn provide(&self, name: &str) -> Option<&Provide> {
        self.provides.iter().find(|p| p.name == name)
    }
}

/// Package ids and signal names: lowercase words joined by `-`, starting with a letter.
pub fn is_kebab_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|w| {
            let mut c = w.chars();
            matches!(c.next(), Some('a'..='z')) && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            id: "test-sensor".into(),
            name: "Test".into(),
            version: "0.1.0".into(),
            port: PortRequirement::Serial { baud: 9600 },
            provides: vec![Provide::default_for(Capability::LaserScan)],
            usb_hints: Vec::new(),
        }
    }

    #[test]
    fn kebab_names() {
        for ok in ["laser-scan", "pm2p5", "ldrobot-ld19", "a"] {
            assert!(is_kebab_name(ok), "{ok}");
        }
        for bad in ["", "Laser", "laser_scan", "-x", "x-", "2d", "a--b", "a-2"] {
            assert!(!is_kebab_name(bad), "{bad}");
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert_eq!(manifest().validate(), Ok(()));
        assert_eq!(manifest().provide("laser-scan").unwrap().capability, Capability::LaserScan);
    }

    #[test]
    fn manifest_errors() {
        let mut m = manifest();
        m.id = "Bad Id".into();
        assert_eq!(m.validate(), Err(ManifestError::BadId("Bad Id".into())));

        let mut m = manifest();
        m.provides.clear();
        assert_eq!(m.validate(), Err(ManifestError::NothingProvided));

        let mut m = manifest();
        m.provides.push(Provide::default_for(Capability::LaserScan));
        assert_eq!(m.validate(), Err(ManifestError::DuplicateProvideName("laser-scan".into())));

        let mut m = manifest();
        m.provides[0].name = "front_scan".into();
        assert_eq!(m.validate(), Err(ManifestError::BadProvideName("front_scan".into())));
    }
}
