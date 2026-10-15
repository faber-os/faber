//! The Faber connector interface.
//!
//! A connector turns one kind of hardware into capabilities. It never does I/O itself: the core
//! opens the port and hands the connector **bytes in**; the connector hands back **readings out**
//! through a [`Context`]. That keeps connectors pure, testable without hardware, and ready to be
//! compiled to WebAssembly (Phase 3) with the same shape.
//!
//! See docs/architecture.md §4 and docs/development.md.

pub mod capture;
pub mod manifest_file;
pub mod settings;
pub mod testing;

pub use settings::{Choice, Number, SettingKind, SettingSpec};

use faber_capabilities::{Capability, Command, Info, Reading, Timestamp};
use std::sync::Arc;

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
    /// Every setting a part of this kind takes, in the order a form shows them
    /// ([`settings`]). Empty: it takes none.
    pub settings: Vec<SettingSpec>,
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
    /// Hardware reached through a FaberLink bridge (a microcontroller on a serial port), such as
    /// motors and drive bases. Bridges arrive in Phase 4; until then such a part can only be
    /// simulated (`simulate = "manual"` or `"world"` in parts.toml).
    Bridge,
}

/// One capability instance a part offers. Its signal address is `<part>/<name>`.
#[derive(Clone, Debug, PartialEq)]
pub struct Provide {
    pub name: String,
    pub capability: Capability,
    /// What the part reports about this capability once: its limits (range, top speed, …). The
    /// core clamps commands to them. Must be for `capability`.
    pub info: Option<Info>,
}

impl Provide {
    /// Uses the capability's own name, the default (docs/capabilities.md, rule 8).
    pub fn default_for(capability: Capability) -> Provide {
        Provide { name: capability.name().to_string(), capability, info: None }
    }

    /// The capability's own name, with its info.
    pub fn with_info(info: Info) -> Provide {
        let capability = info.capability();
        Provide { name: capability.name().to_string(), capability, info: Some(info) }
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

/// What a part did with a command: the command as it was applied (after clamping).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Applied {
    pub command: Command,
}

/// Why a part didn't apply a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    /// This connector doesn't take commands (or not this one).
    Unsupported,
    /// The part can't act right now (still starting, its port is gone, …).
    NotReady(Problem),
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

    /// A command for an actuator. The core has already checked the lease and clamped the command
    /// to the part's info, and it sends the capability's safe state the same way whenever a lease
    /// ends. The default is "not supported": sensors take no commands.
    fn command(&mut self, ctx: &mut dyn Context, command: Command) -> Result<Applied, CommandError> {
        let _ = (ctx, command);
        Err(CommandError::Unsupported)
    }
}

/// A setting that can't be used, with a message and a hint that say how to fix it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError {
    pub key: String,
    pub message: String,
    pub hint: String,
}

/// Makes a running connector for one part's settings.
pub type CreateFn = Arc<dyn Fn(&Settings) -> Result<Box<dyn Connector>, SettingsError> + Send + Sync>;

/// What one part provides with its settings.
pub type ProvidesFn = Arc<dyn Fn(&Manifest, &Settings) -> Result<Vec<Provide>, SettingsError> + Send + Sync>;

/// How a connector runs (docs/packages.md §2). Every kind behaves the same to the core; the kind
/// is shown so people know what they trust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorKind {
    /// Rust compiled into the core.
    BuiltIn,
    /// Only a `faber.toml`; the core's engine runs it.
    Declarative,
    /// A WebAssembly component, sandboxed inside the core.
    Wasm,
}

impl ConnectorKind {
    /// The kind as manifests and the API write it.
    pub fn id(self) -> &'static str {
        match self {
            ConnectorKind::BuiltIn => "built-in",
            ConnectorKind::Declarative => "declarative",
            ConnectorKind::Wasm => "wasm",
        }
    }
}

/// Everything the core needs to offer and start a connector.
///
/// `create` and `provides` are shared closures, so a connector that is data (a declarative
/// manifest, a wasm module) can carry it into them.
#[derive(Clone)]
pub struct Definition {
    pub manifest: Manifest,
    pub kind: ConnectorKind,
    pub create: CreateFn,
    /// What one part provides with these settings. Most connectors provide what their manifest
    /// says ([`manifest_provides`]); generic ones take their limits from the settings.
    pub provides: ProvidesFn,
}

impl Definition {
    /// A built-in connector that always provides what its manifest says.
    pub fn new(
        manifest: Manifest,
        create: impl Fn(&Settings) -> Result<Box<dyn Connector>, SettingsError> + Send + Sync + 'static,
    ) -> Self {
        Definition {
            manifest,
            kind: ConnectorKind::BuiltIn,
            create: Arc::new(create),
            provides: Arc::new(manifest_provides),
        }
    }

    /// The same connector, providing what `provides` says for each part's settings.
    pub fn with_provides(
        self,
        provides: impl Fn(&Manifest, &Settings) -> Result<Vec<Provide>, SettingsError> + Send + Sync + 'static,
    ) -> Self {
        Definition { provides: Arc::new(provides), ..self }
    }

    pub fn with_kind(self, kind: ConnectorKind) -> Self {
        Definition { kind, ..self }
    }
}

impl std::fmt::Debug for Definition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Definition")
            .field("manifest", &self.manifest)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// The default for [`Definition::provides`]: the manifest's own list.
pub fn manifest_provides(manifest: &Manifest, _: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(manifest.provides.clone())
}

/// Why a manifest is not usable.
#[derive(Debug, PartialEq, Eq)]
pub enum ManifestError {
    BadId(String),
    BadProvideName(String),
    DuplicateProvideName(String),
    NothingProvided,
    /// A provide's info is for another capability than the provide's.
    InfoMismatch(String),
    /// A setting's declaration can't be used: why.
    BadSetting(String),
    DuplicateSetting(String),
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
            if p.info.as_ref().is_some_and(|i| i.capability() != p.capability) {
                return Err(ManifestError::InfoMismatch(p.name.clone()));
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        for s in &self.settings {
            if let Some(problem) = s.problem() {
                return Err(ManifestError::BadSetting(problem));
            }
            if !keys.insert(s.key.as_str()) {
                return Err(ManifestError::DuplicateSetting(s.key.clone()));
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
            settings: vec![
                SettingSpec::number("rate-hz", "Rate", "How often it measures.", "Hz", 10.0).positive(),
            ],
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

        let mut m = manifest();
        m.provides[0].info = Some(Info::Estop(faber_capabilities::estop::Info { hardware_latching: false }));
        assert_eq!(m.validate(), Err(ManifestError::InfoMismatch("laser-scan".into())));

        let mut m = manifest();
        m.settings.push(m.settings[0].clone());
        assert_eq!(m.validate(), Err(ManifestError::DuplicateSetting("rate-hz".into())));

        let mut m = manifest();
        m.settings[0].key = "Rate".into();
        assert!(matches!(m.validate(), Err(ManifestError::BadSetting(_))));
    }

    #[test]
    fn info_names_the_provide_after_its_capability() {
        let info =
            Info::LaserScan(faber_capabilities::laser_scan::Info { range_min_m: 0.1, range_max_m: 8.0 });
        let p = Provide::with_info(info.clone());
        assert_eq!(
            (p.name.as_str(), p.capability, p.info),
            ("laser-scan", Capability::LaserScan, Some(info))
        );
    }

    struct Sensor;
    impl Connector for Sensor {
        fn on_bytes(&mut self, _: &mut dyn Context, _: Timestamp, _: &[u8]) {}
    }

    fn create_sensor(_: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
        Ok(Box::new(Sensor))
    }

    #[test]
    fn connectors_take_no_commands_unless_they_say_so() {
        let mut ctx = testing::RecordingContext::new();
        let stop = Command::PlanarBase(faber_capabilities::planar_base::Command::STOP);
        assert_eq!(Sensor.command(&mut ctx, stop), Err(CommandError::Unsupported));
    }

    #[test]
    fn a_definition_provides_what_its_manifest_says_by_default() {
        let d = Definition::new(manifest(), create_sensor);
        assert_eq!((d.provides)(&d.manifest, &Settings::new()).unwrap(), d.manifest.provides);
        assert_eq!(d.kind, ConnectorKind::BuiltIn);
    }

    #[test]
    fn a_definition_can_carry_its_data_into_create_and_provides() {
        let name = String::from("from-data");
        let d = Definition::new(manifest(), create_sensor)
            .with_provides(move |m, _| {
                let mut p = m.provides.clone();
                p[0].name = name.clone();
                Ok(p)
            })
            .with_kind(ConnectorKind::Declarative);
        assert_eq!((d.provides)(&d.manifest, &Settings::new()).unwrap()[0].name, "from-data");
        assert!((d.create)(&Settings::new()).is_ok());
        let copy = d.clone();
        assert_eq!(
            (copy.kind.id(), ConnectorKind::BuiltIn.id(), ConnectorKind::Wasm.id()),
            ("declarative", "built-in", "wasm")
        );
    }
}
