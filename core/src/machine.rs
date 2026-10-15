//! Layer 3: the machine. Ties config, ports, runtime and router together, and is the only
//! place that changes what runs. Everything it does follows one rule (D13): **`parts.toml` is
//! the source of truth**. Changes are written to the file first, then `reconcile` makes the
//! running parts match it.

#[cfg(test)]
mod bridge_tests;
mod flash;
mod packages;
mod safety;
mod world;

pub use flash::{FirmwareSource, FlashProgress, FlashRequest, Flashed};
pub use packages::{PackagePreview, PackageView, installed_connectors};
pub use safety::{CommandReply, EstopView, LeaseEvent};
pub use world::{DEFAULT_WORLD, WorldChanged, WorldView};

use crate::backup::{self, Archive, BackupError, Item};
use crate::bridge::{
    self, BoardView, BridgeDeps, BridgeEvent, BridgeHandle, BridgeOutlet, BridgeSpec, ProbeOutcome,
};
use crate::clock::SharedClock;
use crate::config::{self, BRIDGE_CONNECTOR, ConfigError, PartConfig, PartsConfig, Simulate, Source};
use crate::connectors::LoadProblem;
use crate::disks::{Disk, DiskDiscovery};
use crate::estop::Estop;
use crate::files::{self, Entry, FileError, Files, Special, VPath, WriteMode};
use crate::host;
use crate::leases::{EndReason, Leases};
use crate::names::{PartName, SignalAddress};
use crate::ports::{
    Claimant, Claims, PortEntry, PortEvent, PortId, PortOpener, PortProblem, PortReader, PortWriter,
    ReplayLibrary, SAMPLE_LD19_BYTES, SAMPLE_LD19_NAME, SerialDiscovery, UsbProblemDiscovery, catalog,
    port_problem, spawn_reader,
};
use crate::router::Router;
use crate::runtime::{self, PartStatus, RunDeps, RunSpec, StatusBoard, Timing};
use crate::settings::{
    self as settings_file, DeveloperChange, Settings as MachineSettings, SettingsError, SettingsStore,
};
use crate::simparts::{self, WorldEngine};
use crate::simulation::{self, Outlet, PartSpec};
use crate::suggest::{Suggestion, suggestions};
use crate::translations::Translations;
use faber_capabilities::{Capability, Info, Mount, Reading, estop};
use faber_connector::{
    ConnectorKind, Definition, Health, Manifest, PortRequirement, Problem, Provide, Settings,
};
use safety::Runner;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::{broadcast, watch};

const SERIAL_MONITOR: &str = "serial-monitor";
/// Holds a port while Faber checks it for a FaberLink board.
const BOARD_CHECK: &str = "board-check";
const NEW_FILE_HEADER: &str = "# The parts on this Faber machine.\n# Add and remove parts in the shell, or edit this file by hand: Faber keeps your comments.\n";

pub struct MachineOptions {
    pub data_dir: PathBuf,
    pub clock: SharedClock,
    pub discovery: Arc<dyn SerialDiscovery>,
    /// Finds things plugged in over USB that the computer can't talk to.
    pub usb_problems: Arc<dyn UsbProblemDiscovery>,
    /// Finds USB sticks and SD cards, so Files can show them.
    pub disks: Arc<dyn DiskDiscovery>,
    pub opener: Arc<dyn PortOpener>,
    pub replays: ReplayLibrary,
    /// The connectors that come with the core (built-in). Those installed in
    /// `<data>/connectors/` are added by the machine itself, and reloaded when a package is
    /// installed or removed.
    pub connectors: Vec<Definition>,
    pub timing: Timing,
    /// How often to look for new or removed ports. `None` = only when asked (tests).
    pub port_scan: Option<Duration>,
    /// Runs `simulate = "world"` parts: the world engine ([`simparts::SimWorld`]).
    pub world: Arc<dyn WorldEngine>,
}

/// Why the machine refused a request, in words the user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError {
    pub kind: ErrorKind,
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    /// Why the lease ended, on `lease-ended`, so a program needn't read the message.
    pub lease_ended: Option<EndReason>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    NotFound,
    Conflict,
    /// Something on the machine itself is broken (e.g. the config can't be written).
    Internal,
    /// Understood, but the content is wrong (e.g. a parts.toml with an error).
    Unprocessable,
    /// Not allowed with the current settings, or without a (valid) lease.
    Forbidden,
    /// Locked by the e-stop.
    Locked,
    /// The disk is too full for it.
    InsufficientStorage,
    /// The request is larger than the core accepts (a package file).
    TooLarge,
}

impl From<FileError> for MachineError {
    fn from(e: FileError) -> Self {
        let kind = match e.code {
            "not-found" => ErrorKind::NotFound,
            "already-exists" | "protected" | "no-trash" => ErrorKind::Conflict,
            "machine-files-off" => ErrorKind::Forbidden,
            "io-error" => ErrorKind::Internal,
            _ => ErrorKind::BadRequest,
        };
        MachineError { kind, code: e.code, message: e.message, hint: e.hint, lease_ended: None }
    }
}

impl From<SettingsError> for MachineError {
    fn from(e: SettingsError) -> Self {
        let kind = if e.code == "settings-not-saved" { ErrorKind::Internal } else { ErrorKind::BadRequest };
        MachineError { kind, code: e.code, message: e.message, hint: e.hint, lease_ended: None }
    }
}

impl From<BackupError> for MachineError {
    fn from(e: BackupError) -> Self {
        let kind = if e.code == "backup-invalid" { ErrorKind::Unprocessable } else { ErrorKind::Internal };
        MachineError { kind, code: e.code, message: e.message, hint: e.hint, lease_ended: None }
    }
}

impl MachineError {
    fn new(kind: ErrorKind, code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        MachineError { kind, code, message: message.into(), hint: hint.into(), lease_ended: None }
    }
}

/// A part as the shell shows it. A bridge's sub-parts follow it in [`Machine::parts`].
#[derive(Clone, Debug, PartialEq)]
pub struct PartView {
    pub name: PartName,
    pub connector: String,
    /// A sub-part shows its bridge's source: the board's port.
    pub source: Source,
    pub mount: Mount,
    pub status: PartStatus,
    pub signals: Vec<SignalView>,
    /// For a sub-part: the bridge it is on.
    pub bridge: Option<PartName>,
    /// For a bridge: its board and the link to it.
    pub board: Option<BoardView>,
}

/// What checking a port for a FaberLink board found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardCheck {
    Found {
        board: String,
        firmware: String,
        firmware_version: String,
        parts: u8,
    },
    /// A FaberLink board that speaks another protocol version; the message says what to update.
    OtherVersion {
        message: String,
    },
    Nothing,
}

/// One signal of a part, with its info (limits) as the part's own settings give it. `info` is
/// `None` when the settings can't be read (the part is invalid) or the capability has none.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalView {
    pub address: SignalAddress,
    pub capability: Capability,
    pub info: Option<Info>,
}

/// A port as the shell shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PortView {
    pub entry: PortEntry,
    pub claimed_by: Option<String>,
    pub suggestions: Vec<Suggestion>,
    /// The Arduino board its USB ids name, for the flasher.
    pub board: faber_flash::Detected,
}

/// What the data folder's disk holds (`GET /api/v0/storage`), in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Storage {
    pub recordings: u64,
    pub documents: u64,
    pub desktop: u64,
    pub backups: u64,
    pub free: u64,
    pub total: u64,
}

/// Facts for Settings → About (`GET /api/v0/about`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct About {
    pub data_dir: PathBuf,
    /// Since the core started.
    pub uptime_us: u64,
    pub os: String,
    /// The machine's own IANA time zone.
    pub time_zone: String,
}

/// A request to add a part.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPart {
    pub name: String,
    pub connector: String,
    /// The real part's port. Exactly one of `port` and `simulate`.
    pub port: Option<String>,
    pub simulate: Option<Simulate>,
    pub mount: Mount,
    pub settings: Settings,
}

struct Running {
    config: PartConfig,
    /// `None` when the part could not start (invalid or in conflict).
    runner: Option<Runner>,
    /// What it provides with its settings, with info (limits). Empty if its connector is unknown.
    provides: Vec<Provide>,
}

struct State {
    text: String,
    config: PartsConfig,
    /// Set when parts.toml can't be read; then nothing runs and nothing is written.
    config_problem: Option<ConfigError>,
    ports: Vec<PortEntry>,
    port_problems: Vec<PortProblem>,
    disks: Vec<Disk>,
    claims: Claims,
    running: BTreeMap<PartName, Running>,
    leases: Leases,
    estop: Estop,
    /// The `[world]` loaded into the world engine.
    world: world::Loaded,
    /// FaberLink boards found by checking a port, until the port goes away.
    board_checks: BTreeMap<PortId, BoardCheck>,
}

pub struct Machine {
    clock: SharedClock,
    router: Arc<Router>,
    status: Arc<StatusBoard>,
    discovery: Arc<dyn SerialDiscovery>,
    usb_problems: Arc<dyn UsbProblemDiscovery>,
    disks: Arc<dyn DiskDiscovery>,
    opener: Arc<dyn PortOpener>,
    replays: ReplayLibrary,
    /// The connectors that come with the core ([`MachineOptions::connectors`]).
    base_connectors: Vec<Definition>,
    /// Those plus the installed ones. Replaced when a package is installed or removed; taken
    /// after the state lock, never before it.
    connectors: RwLock<Vec<Definition>>,
    /// Installed connectors that couldn't be loaded, and why.
    connector_problems: Mutex<Vec<LoadProblem>>,
    /// One package install or removal at a time.
    package_lock: Mutex<()>,
    timing: Timing,
    data_dir: PathBuf,
    config_path: PathBuf,
    files: Files,
    settings: SettingsStore,
    translations: Translations,
    state: Mutex<State>,
    ports_changed: watch::Sender<u64>,
    packages_changed: watch::Sender<u64>,
    files_changed: broadcast::Sender<Vec<String>>,
    stop_scanning: Arc<AtomicBool>,
    scanner: Mutex<Option<JoinHandle<()>>>,
    world: Arc<dyn WorldEngine>,
    lease_events: broadcast::Sender<LeaseEvent>,
    estop_events: broadcast::Sender<estop::Reading>,
    world_events: broadcast::Sender<WorldChanged>,
    flash_events: broadcast::Sender<FlashProgress>,
    /// What bridges need the machine to do. Their threads send without waiting; the sweeper
    /// handles it, so a bridge never waits for the machine's lock.
    bridge_events: mpsc::Sender<BridgeEvent>,
    bridge_inbox: Mutex<mpsc::Receiver<BridgeEvent>>,
    stop_sweeping: Arc<AtomicBool>,
    sweeper: Mutex<Option<JoinHandle<()>>>,
}

impl Machine {
    /// Reads `parts.toml` (creating it on first run), finds the ports and starts the parts.
    pub fn start(opts: MachineOptions) -> io::Result<Arc<Machine>> {
        let config_dir = opts.data_dir.join("config");
        std::fs::create_dir_all(&config_dir)?;
        let config_path = config_dir.join(config::FILE_NAME);
        if !config_path.exists() {
            write_atomically(&config_path, NEW_FILE_HEADER)?;
        }
        let text = std::fs::read_to_string(&config_path)?;
        let (config, config_problem) = match world::read_config(&text) {
            Ok(c) => (c, None),
            Err(e) => {
                tracing::error!(target: "core::machine", "parts.toml can't be read: {} {}", e.message, e.hint);
                (PartsConfig::default(), Some(e))
            }
        };
        let files = Files::open(opts.data_dir.join("files"), config_path.clone())?;
        let settings = SettingsStore::load(&config_dir);
        let translations = Translations::open(&config_dir);
        let router = Arc::new(Router::new());
        opts.world.attach(Arc::clone(&router));
        let (connectors, connector_problems) =
            installed_connectors(opts.connectors.clone(), &opts.data_dir.join("connectors"));
        let (bridge_events, bridge_inbox) = mpsc::channel();
        let machine = Arc::new(Machine {
            clock: opts.clock,
            router,
            status: Arc::new(StatusBoard::default()),
            discovery: opts.discovery,
            usb_problems: opts.usb_problems,
            disks: opts.disks,
            opener: opts.opener,
            replays: opts.replays,
            base_connectors: opts.connectors,
            connectors: RwLock::new(connectors),
            connector_problems: Mutex::new(connector_problems),
            package_lock: Mutex::new(()),
            timing: opts.timing,
            data_dir: opts.data_dir.clone(),
            config_path,
            files,
            settings,
            translations,
            state: Mutex::new(State {
                text,
                config,
                config_problem,
                ports: Vec::new(),
                port_problems: Vec::new(),
                disks: Vec::new(),
                claims: Claims::default(),
                running: BTreeMap::new(),
                leases: Leases::new(lease_seed()),
                estop: Estop::default(),
                world: world::Loaded::default(),
                board_checks: BTreeMap::new(),
            }),
            ports_changed: watch::channel(0).0,
            packages_changed: watch::channel(0).0,
            files_changed: broadcast::channel(64).0,
            stop_scanning: Arc::new(AtomicBool::new(false)),
            scanner: Mutex::new(None),
            world: opts.world,
            lease_events: broadcast::channel(256).0,
            estop_events: broadcast::channel(64).0,
            world_events: broadcast::channel(16).0,
            flash_events: broadcast::channel(256).0,
            bridge_events,
            bridge_inbox: Mutex::new(bridge_inbox),
            stop_sweeping: Arc::new(AtomicBool::new(false)),
            sweeper: Mutex::new(None),
        });
        machine.refresh_ports();
        let weak = Arc::downgrade(&machine);
        let stop = Arc::clone(&machine.stop_sweeping);
        let sweeper = std::thread::Builder::new()
            .name("lease-sweeper".into())
            .spawn(move || safety::sweep_leases(weak, stop))
            .expect("spawn lease sweeper");
        *machine.sweeper.lock().expect("sweeper lock") = Some(sweeper);
        if let Some(every) = opts.port_scan {
            let weak = Arc::downgrade(&machine);
            let stop = Arc::clone(&machine.stop_scanning);
            let handle = std::thread::Builder::new()
                .name("port-scan".into())
                .spawn(move || scan_ports(weak, stop, every))
                .expect("spawn port scanner");
            *machine.scanner.lock().expect("scanner lock") = Some(handle);
        }
        Ok(machine)
    }

    /// Ends every lease (sending the safe states), then stops every part, the port scanner and
    /// the lease sweeper.
    pub fn shutdown(&self) {
        self.stop_scanning.store(true, Ordering::Relaxed);
        self.stop_sweeping.store(true, Ordering::Relaxed);
        for slot in [&self.scanner, &self.sweeper] {
            if let Some(h) = slot.lock().expect("thread slot lock").take() {
                let _ = h.join();
            }
        }
        let mut st = self.state.lock().expect("machine lock");
        self.revoke_all(&mut st, EndReason::Shutdown);
        let names: Vec<_> = st.running.keys().cloned().collect();
        for name in names {
            self.stop_part(&mut st, &name);
        }
    }

    pub fn clock(&self) -> &SharedClock {
        &self.clock
    }

    pub fn router(&self) -> &Arc<Router> {
        &self.router
    }

    /// Fires when any part's status (or the list of parts) changes.
    pub fn parts_changed(&self) -> watch::Receiver<u64> {
        self.status.watch()
    }

    /// Fires when ports appear, vanish, or are claimed or released, and when the list of
    /// [`port_problems`](Machine::port_problems) changes.
    pub fn ports_changed(&self) -> watch::Receiver<u64> {
        self.ports_changed.subscribe()
    }

    pub fn config_problem(&self) -> Option<ConfigError> {
        self.state.lock().expect("machine lock").config_problem.clone()
    }

    pub fn connectors(&self) -> Vec<Manifest> {
        self.connectors.read().expect("connectors lock").iter().map(|d| d.manifest.clone()).collect()
    }

    /// Every connector with how it runs (built-in, declarative, …).
    pub fn connector_kinds(&self) -> Vec<(ConnectorKind, Manifest)> {
        self.connectors
            .read()
            .expect("connectors lock")
            .iter()
            .map(|d| (d.kind, d.manifest.clone()))
            .collect()
    }

    /// Installed connectors that couldn't be loaded ([`crate::connectors::LoadProblem`]), so the
    /// API can say why one is missing.
    pub fn connector_problems(&self) -> Vec<LoadProblem> {
        self.connector_problems.lock().expect("connector problems lock").clone()
    }

    fn definition(&self, id: &str) -> Option<Definition> {
        self.connectors.read().expect("connectors lock").iter().find(|d| d.manifest.id == id).cloned()
    }

    /// Every part in parts.toml, each bridge followed by its sub-parts.
    pub fn parts(&self) -> Vec<PartView> {
        let st = self.state.lock().expect("machine lock");
        let mut out = Vec::new();
        for (name, cfg) in &st.config.parts {
            // What it provides with its own settings, resolved when it was (re)started.
            let own = st.running.get(name).map(|r| r.provides.as_slice()).unwrap_or_default();
            let signals = self
                .definition(&cfg.connector)
                .map(|d| {
                    d.manifest
                        .provides
                        .iter()
                        .map(|p| SignalView {
                            address: SignalAddress::new(name.clone(), p.name.clone()),
                            capability: p.capability,
                            info: own.iter().find(|o| o.name == p.name).and_then(|o| o.info.clone()),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let handle = bridge_handle(&st, name);
            out.push(PartView {
                name: name.clone(),
                connector: cfg.connector.clone(),
                source: cfg.source.clone(),
                mount: cfg.mount,
                status: self.status.get(name).unwrap_or(PartStatus::Starting),
                signals,
                bridge: None,
                board: handle.map(BridgeHandle::view),
            });
            if cfg.connector == BRIDGE_CONNECTOR {
                out.extend(self.sub_part_views(name, cfg, handle));
            }
        }
        out
    }

    /// A bridge's sub-parts: those its board has, then those parts.toml names that it doesn't.
    fn sub_part_views(
        &self,
        bridge: &PartName,
        cfg: &PartConfig,
        handle: Option<&BridgeHandle>,
    ) -> Vec<PartView> {
        let subs = handle.map(BridgeHandle::sub_parts).unwrap_or_default();
        let ready = handle.is_some_and(|h| h.view().link == bridge::LinkState::Ready);
        let view = |name: PartName, status: PartStatus, signals: Vec<SignalView>| PartView {
            mount: cfg.parts.get(name.local()).map(|c| c.mount).unwrap_or_default(),
            name,
            connector: cfg.connector.clone(),
            source: cfg.source.clone(),
            status,
            signals,
            bridge: Some(bridge.clone()),
            board: None,
        };
        let mut out: Vec<PartView> = subs
            .iter()
            .map(|s| {
                let signals = s
                    .provide
                    .iter()
                    .map(|p| SignalView {
                        address: SignalAddress::new(s.name.clone(), p.name.clone()),
                        capability: p.capability,
                        info: p.info.clone(),
                    })
                    .collect();
                view(s.name.clone(), self.status.get(&s.name).unwrap_or(PartStatus::Starting), signals)
            })
            .collect();
        for local in cfg.parts.keys() {
            let Ok(name) = PartName::sub(bridge, local) else { continue };
            if subs.iter().any(|s| s.name == name) {
                continue;
            }
            let status = if ready {
                PartStatus::Degraded(Problem::new(
                    format!("Not on the board: \"{bridge}\" has no part called \"{local}\"."),
                    format!(
                        "Check the part's name in the board's sketch, or remove [parts.{bridge}.parts.{local}] from parts.toml."
                    ),
                ))
            } else {
                PartStatus::Starting
            };
            out.push(view(name, status, Vec::new()));
        }
        out
    }

    pub fn ports(&self) -> Vec<PortView> {
        let st = self.state.lock().expect("machine lock");
        let manifests = self.connectors();
        st.ports
            .iter()
            .map(|entry| {
                let recorded_with = match &entry.id {
                    PortId::Replay(name) => self.replays.get(name).and_then(|r| r.recorded_with),
                    _ => None,
                };
                let mut suggestions = suggestions(entry, recorded_with.as_deref(), &manifests);
                if let Some(found) = st.board_checks.get(&entry.id).and_then(board_suggestion) {
                    suggestions.insert(0, found);
                }
                PortView {
                    entry: entry.clone(),
                    claimed_by: st.claims.holder(&entry.id).map(|c| c.to_string()),
                    suggestions,
                    board: flash::guess(entry),
                }
            })
            .collect()
    }

    /// Things plugged in over USB that the computer can't talk to, so they never became a port.
    pub fn port_problems(&self) -> Vec<PortProblem> {
        self.state.lock().expect("machine lock").port_problems.clone()
    }

    /// Looks for new or removed ports (and USB problems and disks), and starts parts whose port
    /// just appeared. Asks the OS before taking the lock, so a slow OS never blocks the machine.
    pub fn refresh_ports(&self) {
        self.refresh_disks();
        self.replays.sync_dir(&self.recordings_dir());
        let found = catalog(&self.discovery.list(), &self.replays.names());
        let mut failed = self.usb_problems.list();
        failed.sort(); // the OS's order isn't stable; only a real change may send `ports`
        let problems: Vec<PortProblem> = failed.iter().filter_map(port_problem).collect();
        let mut st = self.state.lock().expect("machine lock");
        st.board_checks.retain(|id, _| found.iter().any(|p| &p.id == id));
        if st.ports != found || st.port_problems != problems {
            st.ports = found;
            st.port_problems = problems;
            self.ports_changed.send_modify(|n| *n += 1);
        }
        self.reconcile(&mut st);
    }

    /// A disk that was plugged in or pulled out is announced as its root folder (`disk-e:/`)
    /// changing, which tells Files to fetch the list of places again.
    fn refresh_disks(&self) {
        let found = self.disks.list();
        let changed: Vec<VPath> = {
            let mut st = self.state.lock().expect("machine lock");
            if st.disks == found {
                return;
            }
            let ids = |d: &[Disk]| d.iter().map(|d| d.id.clone()).collect::<std::collections::BTreeSet<_>>();
            let changed = ids(&st.disks)
                .symmetric_difference(&ids(&found))
                .filter_map(|id| VPath::parse(&format!("{id}:/")).ok())
                .collect();
            self.files.set_disks(found.iter().map(|d| (d.id.clone(), d.mount.clone())));
            st.disks = found;
            changed
        };
        if !changed.is_empty() {
            self.notify_folders(&changed);
        }
    }

    pub fn add_part(&self, new: NewPart) -> Result<PartView, MachineError> {
        let name = PartName::parse(&new.name)
            .map_err(|b| MachineError::new(ErrorKind::BadRequest, "invalid-name", b.message, b.hint))?;
        if name.as_str() == simparts::WORLD_PART {
            return Err(MachineError::new(
                ErrorKind::BadRequest,
                "invalid-name",
                "\"world\" is the simulated world's own name (its signal is world/pose).",
                "Pick another name, e.g. world_lidar.",
            ));
        }
        let mut st = self.state.lock().expect("machine lock");
        refuse_if_config_broken(&st)?;
        if st.config.parts.contains_key(&name) {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "name-taken",
                format!("There is already a part called \"{name}\"."),
                "Pick another name, or remove the existing part first.",
            ));
        }
        let definition = self.definition(&new.connector).ok_or_else(|| {
            MachineError::new(
                ErrorKind::BadRequest,
                "unknown-connector",
                format!("There is no connector called \"{}\".", new.connector),
                "Pick one of the connectors in the list.",
            )
        })?;
        let invalid_settings = |e: faber_connector::SettingsError| {
            MachineError::new(
                ErrorKind::BadRequest,
                "invalid-settings",
                format!("Setting \"{}\": {}", e.key, e.message),
                e.hint,
            )
        };
        (definition.create)(&new.settings).map_err(invalid_settings)?;
        (definition.provides)(&definition.manifest, &new.settings).map_err(invalid_settings)?;
        let source = match (new.port, new.simulate) {
            (Some(_), Some(_)) => {
                return Err(MachineError::new(
                    ErrorKind::BadRequest,
                    "port-and-simulate",
                    "A part either has a port or is simulated, not both.",
                    "Leave out the port to simulate the part, or leave out simulate to use the real part.",
                ));
            }
            (None, Some(_)) if new.connector == BRIDGE_CONNECTOR => {
                let p = bridge_not_simulated();
                return Err(MachineError::new(
                    ErrorKind::BadRequest,
                    "bridge-not-simulated",
                    p.message,
                    p.hint,
                ));
            }
            (None, Some(mode)) => {
                if mode == Simulate::World
                    && provides_base(&definition.manifest.provides)
                    && let Some(other) = self.world_base(&st, None)
                {
                    return Err(MachineError::new(
                        ErrorKind::Conflict,
                        "world-has-a-base",
                        format!(
                            "\"{other}\" is already the simulated base in the world; there can only be one."
                        ),
                        "The world's base is the machine itself. Simulate this one with simulate = \"manual\", or remove the other first.",
                    ));
                }
                Source::Simulated(mode)
            }
            (port, None) => {
                if definition.manifest.port == PortRequirement::Bridge {
                    let p = runtime::needs_bridge(&definition.manifest);
                    return Err(MachineError::new(ErrorKind::BadRequest, "needs-bridge", p.message, p.hint));
                }
                let port_name = port.unwrap_or_default();
                let port = PortId::parse(&port_name)
                    .and_then(|id| st.ports.iter().find(|p| p.id == id).cloned())
                    .ok_or_else(|| {
                        MachineError::new(
                            ErrorKind::BadRequest,
                            "unknown-port",
                            format!("There is no port called \"{port_name}\" right now."),
                            "Check that it is plugged in, then pick it from the list of ports. Or tick Simulated to try the part without hardware.",
                        )
                    })?;
                if let Some(holder) = st.claims.holder(&port.id) {
                    return Err(port_claimed(&port, &holder.to_string()));
                }
                Source::Port(port.id.to_string())
            }
        };
        let cfg = PartConfig::new(new.connector, source, new.mount, new.settings);
        let text = config::add_part(&st.text, &name, &cfg).map_err(config_write_error)?;
        self.save(&mut st, text)?;
        self.reconcile(&mut st);
        drop(st);
        Ok(self.parts().into_iter().find(|p| p.name == name).expect("just added"))
    }

    pub fn remove_part(&self, name: &str) -> Result<(), MachineError> {
        let unknown = || {
            MachineError::new(
                ErrorKind::NotFound,
                "unknown-part",
                format!("There is no part called \"{name}\"."),
                "Refresh the parts list; it may have been removed already.",
            )
        };
        let name = PartName::parse_any(name).map_err(|_| unknown())?;
        let mut st = self.state.lock().expect("machine lock");
        refuse_if_config_broken(&st)?;
        if let Some(bridge) = name.bridge() {
            return self.remove_sub_part(&mut st, &bridge, &name).map_err(|e| e.unwrap_or_else(unknown));
        }
        if !st.config.parts.contains_key(&name) {
            return Err(unknown());
        }
        let text = config::remove_part(&st.text, &name).map_err(config_write_error)?;
        self.save(&mut st, text)?;
        self.reconcile(&mut st);
        Ok(())
    }

    /// Removes what parts.toml says about a sub-part. The board decides which parts it has, so a
    /// sub-part it still has can't be removed; one it doesn't have (a stale entry) can.
    /// `Err(None)`: there is no such part.
    fn remove_sub_part(
        &self,
        st: &mut State,
        bridge: &PartName,
        name: &PartName,
    ) -> Result<(), Option<MachineError>> {
        let configured = st.config.parts.get(bridge).is_some_and(|c| c.parts.contains_key(name.local()));
        let on_board = bridge_handle(st, bridge).and_then(|h| h.sub_part(name)).is_some();
        if on_board {
            return Err(Some(MachineError::new(
                ErrorKind::Conflict,
                "part-on-board",
                format!(
                    "\"{name}\" is a part on the board \"{bridge}\", so Faber can't remove it on its own."
                ),
                format!(
                    "Take it out of the board's sketch and upload it again, or remove \"{bridge}\" to remove the whole board."
                ),
            )));
        }
        if !configured {
            return Err(None);
        }
        let text = config::remove_sub_part(&st.text, bridge, name.local())
            .map_err(|e| Some(config_write_error(e)))?;
        self.save(st, text).map_err(Some)?;
        self.reconcile(st);
        Ok(())
    }

    /// Writes the new config text, then adopts it. The file is written first, so the running
    /// machine never disagrees with what is on disk.
    fn save(&self, st: &mut State, text: String) -> Result<(), MachineError> {
        let parsed = world::read_config(&text).map_err(config_write_error)?;
        write_atomically(&self.config_path, &text).map_err(|e| {
            MachineError::new(
                ErrorKind::Internal,
                "config-not-saved",
                format!("Could not save parts.toml: {e}"),
                "Check that the data folder exists and is writable, and that the disk isn't full.",
            )
        })?;
        st.text = text;
        st.config = parsed;
        self.notify_files(&[VPath::home(&["System", "parts.toml"])]);
        Ok(())
    }

    /// Makes the running parts match the config: stops removed or changed parts, starts new
    /// ones, and retries parts whose port was missing or busy.
    fn reconcile(&self, st: &mut State) {
        self.sync_world(st);
        let gone: Vec<PartName> = st
            .running
            .iter()
            .filter(|(name, r)| st.config.parts.get(*name).is_none_or(|c| !c.runs_like(&r.config)))
            .map(|(name, _)| name.clone())
            .collect();
        for name in gone {
            let reason = if st.config.parts.contains_key(&name) {
                EndReason::PartRestarted
            } else {
                EndReason::PartRemoved
            };
            self.end_part(st, &name, reason);
            self.stop_part(st, &name);
        }
        // Only the sub-parts' mounts changed: the bridge keeps running.
        for (name, r) in st.running.iter_mut() {
            if let Some(cfg) = st.config.parts.get(name) {
                r.config = cfg.clone();
            }
        }
        let parts: Vec<(PartName, PartConfig)> =
            st.config.parts.iter().map(|(n, c)| (n.clone(), c.clone())).collect();
        for (name, cfg) in parts {
            let retry = st.running.get(&name).is_some_and(|r| r.runner.is_none());
            if st.running.contains_key(&name) && !retry {
                continue;
            }
            st.running.remove(&name);
            let (runner, provides) = self.try_start(st, &name, &cfg);
            st.running.insert(name, Running { config: cfg, runner, provides });
        }
        self.tell_default_world(st);
    }

    /// The part simulated as the base in the world (the machine), other than `except`.
    fn world_base(&self, st: &State, except: Option<&PartName>) -> Option<PartName> {
        st.running
            .iter()
            .filter(|(n, r)| {
                Some(*n) != except
                    && r.runner.is_some()
                    && r.config.source == Source::Simulated(Simulate::World)
                    && provides_base(&r.provides)
            })
            .map(|(n, _)| n.clone())
            .next()
    }

    /// Starts a part. Returns its runner (`None` if it can't run now) and what it provides.
    fn try_start(&self, st: &mut State, name: &PartName, cfg: &PartConfig) -> (Option<Runner>, Vec<Provide>) {
        let Some(definition) = self.definition(&cfg.connector) else {
            self.status.set(
                name,
                PartStatus::Invalid(Problem::new(
                    format!("There is no connector called \"{}\".", cfg.connector),
                    "Fix the connector name in parts.toml, or remove the part and add it again.",
                )),
            );
            return (None, Vec::new());
        };
        let provides = match (definition.provides)(&definition.manifest, &cfg.settings) {
            Ok(p) => p,
            Err(e) => {
                self.status.set(
                    name,
                    PartStatus::Invalid(Problem::new(
                        format!("Setting \"{}\": {}", e.key, e.message),
                        e.hint,
                    )),
                );
                return (None, Vec::new());
            }
        };
        let runner = match cfg.source {
            Source::Simulated(mode) => self.start_simulated(st, name, cfg, mode, &provides),
            Source::Port(_) => self.start_real(st, name, cfg, &definition),
        };
        (runner, provides)
    }

    fn start_simulated(
        &self,
        st: &State,
        name: &PartName,
        cfg: &PartConfig,
        mode: Simulate,
        provides: &[Provide],
    ) -> Option<Runner> {
        if cfg.connector == BRIDGE_CONNECTOR {
            self.status.set(name, PartStatus::Invalid(bridge_not_simulated()));
            return None;
        }
        if mode == Simulate::World
            && provides_base(provides)
            && let Some(other) = self.world_base(st, Some(name))
        {
            self.status.set(
                name,
                PartStatus::Invalid(Problem::new(
                    format!("\"{other}\" is already the simulated base in the world; there can only be one."),
                    "Set simulate = \"manual\" for this one in parts.toml, or remove one of them.",
                )),
            );
            return None;
        }
        for p in provides {
            self.router.declare(SignalAddress::new(name.clone(), p.name.clone()), p.capability);
        }
        self.status.set(name, PartStatus::Starting);
        let spec = PartSpec {
            name: name.clone(),
            connector: cfg.connector.clone(),
            provides: provides.to_vec(),
            mount: cfg.mount,
            settings: cfg.settings.clone(),
            outlet: Arc::new(PartOutlet {
                part: name.clone(),
                router: Arc::clone(&self.router),
                status: Arc::clone(&self.status),
            }),
            clock: self.clock.clone(),
        };
        Some(Runner::Simulated(match mode {
            Simulate::Manual => simulation::start_manual(spec),
            Simulate::World => self.world.start(spec),
        }))
    }

    fn start_real(
        &self,
        st: &mut State,
        name: &PartName,
        cfg: &PartConfig,
        definition: &Definition,
    ) -> Option<Runner> {
        let port_name = match &cfg.source {
            Source::Port(_) if definition.manifest.port == PortRequirement::Bridge => {
                self.status.set(name, PartStatus::Invalid(runtime::needs_bridge(&definition.manifest)));
                return None;
            }
            Source::Port(p) => p.as_str(),
            Source::Simulated(_) => unreachable!("simulated parts start in start_simulated"),
        };
        let Some(port_id) = PortId::parse(port_name) else {
            self.status.set(
                name,
                PartStatus::Invalid(Problem::new(
                    format!("\"{port_name}\" is not a valid port."),
                    "Ports look like usb:10c4:ea60, serial:COM3 or replay:demo-ld19. Fix it in parts.toml.",
                )),
            );
            return None;
        };
        let Some(port) = st.ports.iter().find(|p| p.id == port_id).cloned() else {
            self.status.set(
                name,
                PartStatus::Conflict(Problem::new(
                    format!("Its port {port_name} is not connected."),
                    "Plug it in. Faber starts the part as soon as the port appears.",
                )),
            );
            return None;
        };
        if let Err(crate::ports::ClaimError::Taken(holder)) =
            st.claims.claim(&port.id, Claimant::Part(name.clone()))
        {
            let e = port_claimed(&port, &holder.to_string());
            self.status.set(name, PartStatus::Conflict(Problem::new(e.message, e.hint)));
            return None;
        }
        self.ports_changed.send_modify(|n| *n += 1);
        if definition.manifest.id == BRIDGE_CONNECTOR {
            let deps = BridgeDeps {
                clock: self.clock.clone(),
                opener: Arc::clone(&self.opener),
                router: Arc::clone(&self.router),
                outlet: Arc::new(MachineBridgeOutlet {
                    status: Arc::clone(&self.status),
                    events: Mutex::new(self.bridge_events.clone()),
                }),
                timing: bridge::BridgeTiming {
                    backoff_start: self.timing.backoff_start,
                    retry_max: self.timing.port_retry_max,
                    ..bridge::BridgeTiming::default()
                },
            };
            let estop = st.estop.engaged().map(|r| (r.source, r.reason.clone()));
            let spec = BridgeSpec { name: name.clone(), port };
            return Some(Runner::Bridge(bridge::start(spec, deps, estop)));
        }
        let spec = RunSpec::new(name.clone(), definition, cfg.settings.clone(), port);
        Some(Runner::Real(runtime::start(
            spec,
            RunDeps {
                clock: self.clock.clone(),
                opener: Arc::clone(&self.opener),
                router: Arc::clone(&self.router),
                status: Arc::clone(&self.status),
                timing: self.timing,
            },
        )))
    }

    /// Asks a port whether a FaberLink board is on it (detect, then suggest). Only when someone
    /// asks, and only on a port nobody uses: opening a port resets most Arduino-based devices,
    /// so Faber never does it on its own (docs/faberlink.md §9). The port is held while it is
    /// checked, a few seconds at most, and a board found is suggested for it from then on.
    pub fn check_for_board(&self, port: &str) -> Result<BoardCheck, MachineError> {
        let me = Claimant::Tool(BOARD_CHECK);
        let entry = {
            let mut st = self.state.lock().expect("machine lock");
            let entry = PortId::parse(port)
                .and_then(|id| st.ports.iter().find(|p| p.id == id).cloned())
                .ok_or_else(|| {
                    MachineError::new(
                        ErrorKind::BadRequest,
                        "unknown-port",
                        format!("There is no port called \"{port}\" right now."),
                        "Check that it is plugged in, then pick it from the list of ports.",
                    )
                })?;
            if entry.kind == crate::ports::PortKind::Replay {
                return Err(MachineError::new(
                    ErrorKind::BadRequest,
                    "not-a-serial-port",
                    format!("{} is a recording, so no board can answer on it.", entry.label),
                    "Check a port with a board plugged into it.",
                ));
            }
            if let Err(crate::ports::ClaimError::Taken(holder)) = st.claims.claim(&entry.id, me.clone()) {
                return Err(port_claimed(&entry, &holder.to_string()));
            }
            self.ports_changed.send_modify(|n| *n += 1);
            entry
        };
        let outcome = match self.opener.open(&entry, crate::connectors::bridge::BAUD) {
            Ok((reader, mut writer)) => {
                bridge::probe(reader, &mut *writer, &self.clock, &bridge::ProbeTiming::default())
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        };
        let mut st = self.state.lock().expect("machine lock");
        st.claims.release(&entry.id, &me);
        self.ports_changed.send_modify(|n| *n += 1);
        let check = match outcome {
            Ok(ProbeOutcome::Found(b)) => BoardCheck::Found {
                board: b.board,
                firmware: b.firmware,
                firmware_version: b.firmware_version,
                parts: b.part_count,
            },
            Ok(ProbeOutcome::OtherVersion { message }) => BoardCheck::OtherVersion { message },
            Ok(ProbeOutcome::Nothing) => BoardCheck::Nothing,
            Err(e) => {
                self.reconcile(&mut st);
                return Err(MachineError::new(
                    ErrorKind::Conflict,
                    "port-unavailable",
                    format!("Could not check {}: {e}", entry.label),
                    "Check that no other program (for example the Arduino IDE's serial monitor) uses it.",
                ));
            }
        };
        match &check {
            BoardCheck::Nothing => st.board_checks.remove(&entry.id),
            found => st.board_checks.insert(entry.id.clone(), found.clone()),
        };
        tracing::info!(target: "core::bridge", "checked {} for a FaberLink board: {check:?}", entry.label);
        self.reconcile(&mut st); // a part waiting for this port can start now
        Ok(check)
    }

    fn stop_part(&self, st: &mut State, name: &PartName) {
        if let Some(r) = st.running.remove(name) {
            let simulated = matches!(r.runner, Some(Runner::Simulated(_)));
            if let Some(runner) = r.runner {
                runner.stop();
            }
            if simulated {
                for p in &r.provides {
                    self.router.remove(&SignalAddress::new(name.clone(), p.name.clone()));
                }
            }
            if let Some(id) = r.config.source.port().and_then(PortId::parse) {
                st.claims.release(&id, &Claimant::Part(name.clone()));
                self.ports_changed.send_modify(|n| *n += 1);
            }
        }
        if !st.config.parts.contains_key(name) {
            self.status.remove(name);
        }
    }

    /// Opens and claims a port for the serial monitor. Nothing is read until
    /// [`SerialSession::start`], so the caller can announce the session before data flows.
    pub fn open_serial(self: &Arc<Self>, port: &str, baud: u32) -> Result<SerialSession, MachineError> {
        let mut st = self.state.lock().expect("machine lock");
        let entry = PortId::parse(port)
            .and_then(|id| st.ports.iter().find(|p| p.id == id).cloned())
            .ok_or_else(|| {
                MachineError::new(
                    ErrorKind::BadRequest,
                    "unknown-port",
                    format!("There is no port called \"{port}\" right now."),
                    "Check that it is plugged in, then pick it from the list of ports.",
                )
            })?;
        let me = Claimant::Tool(SERIAL_MONITOR);
        if let Err(crate::ports::ClaimError::Taken(holder)) = st.claims.claim(&entry.id, me.clone()) {
            return Err(port_claimed(&entry, &holder.to_string()));
        }
        let (reader, writer) = match self.opener.open(&entry, baud) {
            Ok(rw) => rw,
            Err(e) => {
                st.claims.release(&entry.id, &me);
                return Err(MachineError::new(
                    ErrorKind::Conflict,
                    "port-unavailable",
                    format!("Could not open {}: {e}", entry.label),
                    "Check that no other program (for example another serial terminal) uses it.",
                ));
            }
        };
        self.ports_changed.send_modify(|n| *n += 1);
        Ok(SerialSession {
            machine: Arc::downgrade(self),
            clock: self.clock.clone(),
            port: entry.id,
            reader: Mutex::new(Some(reader)),
            writer: Mutex::new(writer),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            recorder: Arc::new(Mutex::new(None)),
        })
    }

    fn close_serial(&self, port: &PortId) {
        let mut st = self.state.lock().expect("machine lock");
        st.claims.release(port, &Claimant::Tool(SERIAL_MONITOR));
        self.ports_changed.send_modify(|n| *n += 1);
        self.reconcile(&mut st); // a part waiting for this port can start now
    }

    // ---- settings -------------------------------------------------------------------------

    pub fn settings(&self) -> MachineSettings {
        self.settings.get()
    }

    pub fn settings_changed(&self) -> watch::Receiver<u64> {
        self.settings.watch()
    }

    /// The shell translations installed in `config/translations` (D44).
    pub fn translations(&self) -> &Translations {
        &self.translations
    }

    pub fn update_settings(&self, new: MachineSettings) -> Result<MachineSettings, MachineError> {
        Ok(self.settings.update(new)?)
    }

    /// Every setting back to its default (whole-machine files off too). Parts and files stay.
    pub fn reset_settings(&self) -> Result<MachineSettings, MachineError> {
        let before = self.settings.get().developer.full_machine_files;
        let after = self.settings.reset()?;
        if before {
            tracing::warn!(target: "core::files", "whole-machine file access turned off (settings reset)");
        }
        Ok(after)
    }

    pub fn change_developer(&self, change: DeveloperChange) -> Result<MachineSettings, MachineError> {
        let before = self.settings.get().developer.full_machine_files;
        let after = self.settings.change_developer(change)?;
        if before != after.developer.full_machine_files {
            tracing::warn!(
                target: "core::files",
                "whole-machine file access turned {}",
                if after.developer.full_machine_files { "ON" } else { "off" }
            );
        }
        Ok(after)
    }

    // ---- this computer, storage and backups ----------------------------------------------

    /// The machine's own time zone (D42).
    pub fn time_zone(&self) -> String {
        host::time_zone()
    }

    /// The zone wall-clock times are shown in: the setting, or the machine's own.
    fn display_zone(&self) -> String {
        let zone = self.settings.get().time.time_zone;
        if zone.is_empty() { host::time_zone() } else { zone }
    }

    pub fn about(&self) -> About {
        About {
            // `--data ./data` is shown as the whole path, which is what a helper needs.
            data_dir: std::path::absolute(&self.data_dir).unwrap_or_else(|_| self.data_dir.clone()),
            uptime_us: self.clock.now_us(),
            os: host::os(),
            time_zone: host::time_zone(),
        }
    }

    pub fn storage(&self) -> Result<Storage, MachineError> {
        let space = host::disk_space(&self.data_dir).map_err(|e| {
            MachineError::new(
                ErrorKind::Internal,
                "io-error",
                format!("Couldn't measure the free space on the data folder's disk: {e}"),
                "Check that the data folder's disk is still connected.",
            )
        })?;
        Ok(Storage {
            recordings: self.files.folder_size(Special::Recordings),
            documents: self.files.folder_size(Special::Documents),
            desktop: self.files.folder_size(Special::Desktop),
            backups: self.files.folder_size(Special::Backups),
            free: space.free,
            total: space.total,
        })
    }

    /// Refuses before writing anything when `needed` bytes won't fit on the data folder's disk.
    fn check_room(&self, needed: u64, what: &str, hint: &str) -> Result<(), MachineError> {
        let free = host::disk_space(&self.data_dir).map(|s| s.free).unwrap_or(u64::MAX);
        if needed <= free {
            return Ok(());
        }
        let mb = |b: u64| b.div_ceil(1024 * 1024);
        Err(MachineError::new(
            ErrorKind::InsufficientStorage,
            "disk-full",
            format!("{what} needs about {} MB, but only {} MB is free.", mb(needed), mb(free)),
            hint,
        ))
    }

    /// Saves a backup in `home:/Backups` (D41). Returns its path and size.
    pub fn create_backup(&self, include_recordings: bool) -> Result<(String, u64), MachineError> {
        let home = self.files.home_dir().to_path_buf();
        let settings_path = self.config_path.with_file_name(settings_file::FILE_NAME);
        // No install or uninstall half-way through while the connectors are copied.
        let _one_at_a_time = self.package_lock.lock().expect("package lock");
        let (packages_path, connectors, connector_ids) = self.packages_to_back_up()?;
        let src = backup::Sources {
            parts_toml: &self.config_path,
            settings_toml: &settings_path,
            packages_toml: &packages_path,
            connectors: &connectors,
            connector_ids: &connector_ids,
            home: &home,
            include_recordings,
        };
        self.check_room(
            backup::source_bytes(&src),
            "The backup",
            "Delete old backups or recordings and empty the Trash, or leave the recordings out.",
        )?;
        let folder = home.join(Special::Backups.name());
        let name = files::unique_name(
            &folder,
            &backup::file_name(jiff::Timestamp::now(), &self.display_zone(), &self.settings().machine.name),
        );
        let bytes = backup::create(&src, &folder.join(&name))?;
        let p = VPath::home(&["Backups", &name]);
        tracing::info!(target: "core::backup", "saved the backup {p} ({bytes} bytes)");
        self.notify_files(std::slice::from_ref(&p));
        Ok((p.to_string(), bytes))
    }

    /// Restores a backup from the home folder. Everything in it is checked before anything
    /// changes, so a broken parts.toml is refused with its line and nothing is touched, and every
    /// connector in it is checked and loaded in staging exactly like installing its package.
    /// Files that the backup replaces go to the Trash; files and packages it doesn't have stay.
    pub fn restore_backup(&self, path: &str) -> Result<(), MachineError> {
        let p = VPath::parse(path)?;
        let real = self.files.local_path(&p)?;
        if !real.is_file() {
            return Err(FileError::new_public(
                "not-found",
                format!("{p} doesn't exist."),
                "Refresh the folder; it may have been moved or deleted.",
            )
            .into());
        }
        let mut archive = Archive::open(&real)?;

        // 1. Check everything first.
        world::read_config(archive.parts_toml()).map_err(|e| config_invalid(&e))?;
        let restored_settings = match archive.settings_toml() {
            None => None,
            Some(text) => Some(settings_file::parse(text).map_err(|e| {
                MachineError::new(ErrorKind::Unprocessable, "backup-invalid", e.message, e.hint)
            })?),
        };
        let items = archive
            .items()
            .iter()
            .map(|i| {
                VPath::parse(&format!("home:/{}", i.path)).map(|vp| (vp, i.clone())).map_err(|_| {
                    MachineError::new(
                        ErrorKind::Unprocessable,
                        "backup-invalid",
                        format!("The backup holds \"{}\", which isn't a name Faber can use.", i.path),
                        "Pick a backup Faber made (in home:/Backups, named …-backup-….zip).",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.check_room(
            archive.unpacked_bytes(),
            "Restoring this backup",
            "Delete old backups or recordings and empty the Trash, then try again.",
        )?;
        let _one_at_a_time = self.package_lock.lock().expect("package lock");
        let connectors = self.stage_backup_connectors(&mut archive)?;

        // 2. Then change things: the connectors first, so the parts find them, then the config
        // and settings, as they matter most.
        self.restore_connectors(connectors)?;
        self.write_config_text(archive.parts_toml().as_bytes())?;
        if let Some(s) = restored_settings {
            self.settings.restore(s)?;
        }
        let result = items.iter().try_for_each(|(vp, item)| self.restore_item(&mut archive, vp, item));
        self.notify_folders(&[VPath::home(&[])]);
        self.refresh_ports(); // restored recordings become replay ports
        result?;
        tracing::info!(target: "core::backup", "restored the backup {p}");
        Ok(())
    }

    fn restore_item(&self, archive: &mut Archive, vp: &VPath, item: &Item) -> Result<(), MachineError> {
        let real = self.files.local_path(vp)?;
        let io = |e: io::Error| {
            MachineError::new(
                ErrorKind::Internal,
                "io-error",
                format!("Couldn't restore {vp}: {e}"),
                "Check that the disk has space and the file isn't open elsewhere.",
            )
        };
        if item.folder {
            if real.is_file() {
                self.files.trash(std::slice::from_ref(vp))?;
            }
            return std::fs::create_dir_all(&real).map_err(io);
        }
        if let Some(parent) = real.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let tmp = real.with_file_name(format!(".{}.faber-tmp", vp.name().unwrap_or("file")));
        if let Err(e) = archive.extract(item, &tmp) {
            let _ = std::fs::remove_file(&tmp);
            return Err(io(e));
        }
        if real.is_file() && same_contents(&tmp, &real).map_err(io)? {
            return std::fs::remove_file(&tmp).map_err(io);
        }
        if real.exists() {
            self.files.trash(std::slice::from_ref(vp))?;
        }
        std::fs::rename(&tmp, &real).map_err(io)
    }

    // ---- files ----------------------------------------------------------------------------

    fn machine_files(&self) -> bool {
        self.settings.get().developer.full_machine_files
    }

    fn recordings_dir(&self) -> PathBuf {
        self.files.home_dir().join(files::Special::Recordings.name())
    }

    /// Fires with the folders whose contents changed.
    pub fn files_changed(&self) -> broadcast::Receiver<Vec<String>> {
        self.files_changed.subscribe()
    }

    /// `changed` are the items that changed; their folders are announced.
    fn notify_files(&self, changed: &[VPath]) {
        let folders: Vec<VPath> = changed.iter().map(|p| p.parent().unwrap_or_else(|| p.clone())).collect();
        self.notify_folders(&folders);
    }

    /// Announces that these folders' contents changed.
    fn notify_folders(&self, folders: &[VPath]) {
        let mut folders: Vec<String> = folders.iter().map(VPath::to_string).collect();
        folders.sort();
        folders.dedup();
        let _ = self.files_changed.send(folders);
    }

    /// The places Files shows: home, then connected disks, then (developer setting) the machine.
    pub fn file_roots(&self) -> Vec<(VPath, String, Option<&'static str>)> {
        let mut roots = vec![(VPath::home(&[]), "Home".to_string(), None)];
        let disks = self.state.lock().expect("machine lock").disks.clone();
        roots.extend(
            disks
                .into_iter()
                .filter_map(|d| Some((VPath::parse(&format!("{}:/", d.id)).ok()?, d.label, None))),
        );
        if self.machine_files() {
            roots.push((
                VPath { root: files::RootKey::Machine, parts: vec![] },
                "This computer".to_string(),
                Some("You're browsing the whole computer. Deleting or changing system files can break it, and there's no Trash here."),
            ));
        }
        roots
    }

    pub fn list_files(&self, path: &str) -> Result<Vec<Entry>, MachineError> {
        Ok(self.files.list(&VPath::parse(path)?, self.machine_files())?)
    }

    /// A file to stream out as a download: its path on disk and size. No size limit.
    pub fn download_file(&self, path: &str) -> Result<(VPath, PathBuf, u64), MachineError> {
        let p = VPath::parse(path)?;
        let (real, len) = self.files.locate_file(&p, self.machine_files())?;
        Ok((p, real, len))
    }

    pub fn read_file(&self, path: &str) -> Result<(VPath, Vec<u8>), MachineError> {
        let p = VPath::parse(path)?;
        let bytes = self.files.read(&p, self.machine_files())?;
        Ok((p, bytes))
    }

    /// Writes a file. `home:/System/parts.toml` is the machine config: it is validated first,
    /// then saved and applied, exactly like adding a part in the shell.
    pub fn write_file(&self, path: &str, bytes: &[u8], mode: WriteMode) -> Result<(), MachineError> {
        let p = VPath::parse(path)?;
        if Files::is_config(&p) {
            if mode == WriteMode::CreateOnly {
                return Err(FileError::new_public(
                    "already-exists",
                    format!("{p} already exists."),
                    "Open it and edit it instead.",
                )
                .into());
            }
            return self.write_config_text(bytes);
        }
        self.files.write(&p, bytes, mode, self.machine_files())?;
        self.notify_files(std::slice::from_ref(&p));
        if p.parent().is_some_and(|d| d == VPath::home(&["Recordings"])) {
            self.refresh_ports();
        }
        Ok(())
    }

    fn write_config_text(&self, bytes: &[u8]) -> Result<(), MachineError> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| {
            MachineError::new(
                ErrorKind::Unprocessable,
                "config-invalid",
                "parts.toml must be text (UTF-8).",
                "Save it as plain text.",
            )
        })?;
        world::read_config(&text).map_err(|e| config_invalid(&e))?;
        let mut st = self.state.lock().expect("machine lock");
        self.save(&mut st, text)?;
        st.config_problem = None;
        self.reconcile(&mut st);
        Ok(())
    }

    pub fn create_folder(&self, path: &str) -> Result<(), MachineError> {
        let p = VPath::parse(path)?;
        self.files.create_folder(&p, self.machine_files())?;
        self.notify_files(&[p]);
        Ok(())
    }

    pub fn move_file(&self, from: &str, to: &str) -> Result<(), MachineError> {
        let (from, to) = (VPath::parse(from)?, VPath::parse(to)?);
        self.files.rename(&from, &to, self.machine_files())?;
        self.notify_files(&[from, to]);
        self.refresh_ports(); // a recording may have been renamed or moved
        Ok(())
    }

    fn parse_all(paths: &[String]) -> Result<Vec<VPath>, MachineError> {
        if paths.is_empty() {
            return Err(MachineError::new(
                ErrorKind::BadRequest,
                "bad-path",
                "No paths given.",
                "Pick at least one file.",
            ));
        }
        paths.iter().map(|p| VPath::parse(p).map_err(MachineError::from)).collect()
    }

    pub fn trash_files(&self, paths: &[String]) -> Result<(), MachineError> {
        let ps = Self::parse_all(paths)?;
        self.files.trash(&ps)?;
        self.notify_files(&ps);
        self.notify_folders(&[VPath::home(&["Trash"])]);
        self.refresh_ports();
        Ok(())
    }

    pub fn restore_files(&self, paths: &[String]) -> Result<(), MachineError> {
        let ps = Self::parse_all(paths)?;
        self.files.restore(&ps)?;
        self.notify_folders(&[VPath::home(&["Trash"])]);
        // Restored files go back where they came from, or to Documents.
        self.notify_folders(&[VPath::home(&[])]);
        self.refresh_ports();
        Ok(())
    }

    pub fn delete_files(&self, paths: &[String]) -> Result<(), MachineError> {
        let ps = Self::parse_all(paths)?;
        self.files.delete(&ps, self.machine_files())?;
        self.notify_files(&ps);
        Ok(())
    }

    pub fn empty_trash(&self) -> Result<(), MachineError> {
        self.files.empty_trash()?;
        self.notify_folders(&[VPath::home(&["Trash"])]);
        Ok(())
    }

    /// Copies the sample LD19 recording into Recordings (D32), where it becomes a replay port.
    pub fn add_samples(&self) -> Result<String, MachineError> {
        let p = VPath::home(&["Recordings", &format!("{SAMPLE_LD19_NAME}.fbcap")]);
        if self.files.read(&p, false).is_ok() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-exists",
                "The sample recording is already in Recordings.",
                "Use the port replay:ld19-sample when you add a part.",
            ));
        }
        self.files.write(&p, SAMPLE_LD19_BYTES, WriteMode::CreateOnly, false)?;
        self.notify_files(std::slice::from_ref(&p));
        self.refresh_ports();
        Ok(p.to_string())
    }

    /// Starts recording what `session` receives into `Recordings/<name>.fbcap`.
    pub fn start_recording(&self, session: &SerialSession, name: &str) -> Result<String, MachineError> {
        PartName::parse(name).map_err(|b| {
            MachineError::new(
                ErrorKind::BadRequest,
                "invalid-name",
                b.message.replace("part name", "recording name"),
                b.hint,
            )
        })?;
        let p = VPath::home(&["Recordings", &format!("{name}.fbcap")]);
        let real = self.recordings_dir().join(format!("{name}.fbcap"));
        if real.exists() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-exists",
                format!("{p} already exists."),
                "Pick another name.",
            ));
        }
        let recorder = Recorder::create(&real).map_err(|e| {
            MachineError::new(
                ErrorKind::Internal,
                "io-error",
                format!("Couldn't create {p}: {e}"),
                "Check the disk has space.",
            )
        })?;
        let mut slot = session.recorder.lock().expect("recorder lock");
        if slot.is_some() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-recording",
                "This port is already being recorded.",
                "Stop the current recording first.",
            ));
        }
        *slot = Some((p.to_string(), recorder));
        Ok(p.to_string())
    }

    /// Stops recording; returns the recording's path and how many bytes it holds.
    pub fn stop_recording(&self, session: &SerialSession) -> Result<(String, u64), MachineError> {
        let taken = session.recorder.lock().expect("recorder lock").take();
        let (path, recorder) = taken.ok_or_else(|| {
            MachineError::new(
                ErrorKind::Conflict,
                "not-recording",
                "This port isn't being recorded.",
                "Start a recording first.",
            )
        })?;
        let bytes = recorder.finish().map_err(|e| {
            MachineError::new(
                ErrorKind::Internal,
                "io-error",
                format!("Couldn't finish {path}: {e}"),
                "Check the disk has space.",
            )
        })?;
        if let Ok(p) = VPath::parse(&path) {
            self.notify_files(&[p]);
        }
        self.refresh_ports();
        Ok((path, bytes))
    }
}

/// Writes a recording as chunks arrive (`.fbcap`, docs/api.md).
struct Recorder {
    out: std::io::BufWriter<std::fs::File>,
    bytes: u64,
}

impl Recorder {
    fn create(path: &Path) -> io::Result<Recorder> {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        out.write_all(faber_connector::capture::MAGIC)?;
        Ok(Recorder { out, bytes: 0 })
    }

    fn append(&mut self, t_us: u64, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        self.out.write_all(&t_us.to_le_bytes())?;
        self.out.write_all(&(data.len() as u32).to_le_bytes())?;
        self.out.write_all(data)?;
        self.bytes += data.len() as u64;
        Ok(())
    }

    fn finish(mut self) -> io::Result<u64> {
        use std::io::Write;
        self.out.flush()?;
        Ok(self.bytes)
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        self.stop_scanning.store(true, Ordering::Relaxed);
        self.stop_sweeping.store(true, Ordering::Relaxed);
    }
}

/// Where a simulated part's readings and health go: the router and the status board.
struct PartOutlet {
    part: PartName,
    router: Arc<Router>,
    status: Arc<StatusBoard>,
}

impl Outlet for PartOutlet {
    fn publish(&self, name: &str, reading: Reading) {
        if let Err(e) = self.router.publish(&SignalAddress::new(self.part.clone(), name), reading) {
            tracing::error!(target: "core::simulation", part = %self.part, "a simulated part published wrongly: {e:?}");
        }
    }

    fn set_health(&self, health: Health) {
        let status = match health {
            Health::Starting => PartStatus::Starting,
            Health::Running => PartStatus::Running,
            Health::Degraded(p) => PartStatus::Degraded(p),
            Health::Failed(p) => PartStatus::Failed(p),
        };
        self.status.set(&self.part, status);
    }
}

/// Where a bridge's health and events go: the status board, and the sweeper's inbox.
struct MachineBridgeOutlet {
    status: Arc<StatusBoard>,
    events: Mutex<mpsc::Sender<BridgeEvent>>,
}

impl BridgeOutlet for MachineBridgeOutlet {
    fn set_health(&self, part: &PartName, health: Health) {
        let status = match health {
            Health::Starting => PartStatus::Starting,
            Health::Running => PartStatus::Running,
            Health::Degraded(p) => PartStatus::Degraded(p),
            Health::Failed(p) => PartStatus::Failed(p),
        };
        self.status.set(part, status);
    }

    fn forget(&self, part: &PartName) {
        self.status.remove(part);
    }

    fn event(&self, event: BridgeEvent) {
        let _ = self.events.lock().expect("bridge events lock").send(event);
    }
}

/// A running bridge's handle.
fn bridge_handle<'a>(st: &'a State, bridge: &PartName) -> Option<&'a BridgeHandle> {
    match st.running.get(bridge)?.runner.as_ref()? {
        Runner::Bridge(h) => Some(h),
        _ => None,
    }
}

fn bridge_not_simulated() -> Problem {
    Problem::new(
        "A FaberLink board can't be simulated: it is real hardware that hosts other parts.",
        "Simulate its motors and sensors on their own with the generic connectors (Generic motor, Generic distance sensor), or add the board on its port.",
    )
}

/// "This port has a FaberLink board", once a check found one.
fn board_suggestion(check: &BoardCheck) -> Option<Suggestion> {
    let reason = match check {
        BoardCheck::Found { board, firmware, firmware_version, .. } => {
            format!("A FaberLink board: {board} ({firmware} {firmware_version}).")
        }
        BoardCheck::OtherVersion { message } => format!("A FaberLink board, but: {message}"),
        BoardCheck::Nothing => return None,
    };
    Some(Suggestion { connector: BRIDGE_CONNECTOR.into(), reason })
}

fn provides_base(provides: &[Provide]) -> bool {
    provides.iter().any(|p| p.capability == Capability::PlanarBase)
}

/// Differs between runs of the core, so lease ids do too.
fn lease_seed() -> u64 {
    let nanos =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    (nanos as u64) ^ u64::from(std::process::id()).rotate_left(32)
}

/// A port held open by the serial monitor. Dropping it closes the port and releases the claim.
pub struct SerialSession {
    machine: Weak<Machine>,
    clock: SharedClock,
    port: PortId,
    /// Taken by `start`. In a mutex so a session can be shared between threads.
    reader: Mutex<Option<Box<dyn PortReader>>>,
    writer: Mutex<Box<dyn PortWriter>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The recording in progress, if any: its path and writer.
    recorder: Arc<Mutex<Option<(String, Recorder)>>>,
}

impl SerialSession {
    pub fn port(&self) -> String {
        self.port.to_string()
    }

    /// Starts reading. Events go to `sink` until the session is dropped or `sink` returns false.
    /// Calling it twice does nothing the second time.
    pub fn start(&mut self, mut sink: impl FnMut(PortEvent) -> bool + Send + 'static) {
        let recorder = Arc::clone(&self.recorder);
        let recording_sink = move |event: PortEvent| {
            if let PortEvent::Data { t_us, bytes } = &event
                && let Some((path, rec)) = recorder.lock().expect("recorder lock").as_mut()
                && let Err(e) = rec.append(*t_us, bytes)
            {
                tracing::error!(target: "core::files", "recording to {path} failed: {e}");
            }
            sink(event)
        };
        if let Some(reader) = self.reader.get_mut().expect("reader lock").take() {
            self.thread =
                Some(spawn_reader(reader, self.clock.clone(), recording_sink, Arc::clone(&self.stop)));
        }
    }

    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        self.writer.lock().expect("writer lock").write(data)
    }
}

impl Drop for SerialSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Some(m) = self.machine.upgrade() {
            if self.recorder.lock().expect("recorder lock").is_some() {
                let _ = m.stop_recording(self); // closing the port also ends its recording
            }
            m.close_serial(&self.port);
        }
    }
}

fn scan_ports(machine: Weak<Machine>, stop: Arc<AtomicBool>, every: Duration) {
    loop {
        let mut slept = Duration::ZERO;
        while slept < every {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
            slept += Duration::from_millis(50);
        }
        match machine.upgrade() {
            Some(m) => m.refresh_ports(),
            None => return,
        }
    }
}

fn port_claimed(port: &PortEntry, holder: &str) -> MachineError {
    let (message, hint) = match holder.strip_prefix("tool:") {
        Some(tool) => (
            format!("{} is open in the {}.", port.label, tool.replace('-', " ")),
            "Close it there first, or pick another port.".to_string(),
        ),
        None => (
            format!("{} is used by the part \"{holder}\".", port.label),
            format!("Remove \"{holder}\" first, or pick another port."),
        ),
    };
    MachineError::new(ErrorKind::Conflict, "port-claimed", message, hint)
}

fn refuse_if_config_broken(st: &State) -> Result<(), MachineError> {
    match &st.config_problem {
        None => Ok(()),
        Some(e) => Err(MachineError::new(
            ErrorKind::Conflict,
            "config-broken",
            match e.line {
                Some(l) => format!("parts.toml has a problem on line {l}: {}", e.message),
                None => format!("parts.toml has a problem: {}", e.message),
            },
            "Fix parts.toml and restart Faber. Faber won't change the file until then, so nothing of yours is lost.",
        )),
    }
}

/// A parts.toml that can't be used, refused with the line it goes wrong on.
fn config_invalid(e: &ConfigError) -> MachineError {
    let message = match e.line {
        Some(l) => format!("parts.toml has a problem on line {l}: {}", e.message),
        None => format!("parts.toml has a problem: {}", e.message),
    };
    MachineError::new(ErrorKind::Unprocessable, "config-invalid", message, e.hint.clone())
}

/// True if both files hold the same bytes.
fn same_contents(a: &Path, b: &Path) -> io::Result<bool> {
    use std::io::Read;
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = fa.read(&mut ba)?;
        if n == 0 {
            return Ok(true);
        }
        fb.read_exact(&mut bb[..n])?;
        if ba[..n] != bb[..n] {
            return Ok(false);
        }
    }
}

fn config_write_error(e: ConfigError) -> MachineError {
    MachineError::new(ErrorKind::Internal, "config-not-saved", e.message, e.hint)
}

/// Writes via a temporary file and a rename, so a crash never leaves a half-written file.
fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MonotonicClock;
    use crate::ports::{DEMO_LD19, Discovered, FailedUsb, SystemOpener, Usb};
    use std::time::Instant;

    struct FakeDiscovery(Mutex<Vec<Discovered>>);

    impl SerialDiscovery for FakeDiscovery {
        fn list(&self) -> Vec<Discovered> {
            self.0.lock().unwrap().clone()
        }
    }

    #[derive(Default)]
    struct FakeUsbProblems(Mutex<Vec<FailedUsb>>);

    #[derive(Default)]
    struct FakeDisks(Mutex<Vec<Disk>>);

    impl DiskDiscovery for FakeDisks {
        fn list(&self) -> Vec<Disk> {
            self.0.lock().unwrap().clone()
        }
    }

    impl UsbProblemDiscovery for FakeUsbProblems {
        fn list(&self) -> Vec<FailedUsb> {
            self.0.lock().unwrap().clone()
        }
    }

    fn reset_failed(os_id: &str) -> FailedUsb {
        FailedUsb {
            os_id: os_id.into(),
            hardware_ids: vec![r"USB\RESET_FAILURE".into()],
            problem_code: Some(43),
        }
    }

    fn cp2102() -> Discovered {
        Discovered {
            path: "COM_DOES_NOT_EXIST".into(),
            usb: Some(Usb {
                vid: 0x10c4,
                pid: 0xea60,
                serial_number: None,
                manufacturer: None,
                product: Some("CP2102".into()),
            }),
            bluetooth: false,
        }
    }

    fn machine(dir: &Path, discovery: Arc<FakeDiscovery>) -> Arc<Machine> {
        machine_with(dir, discovery, Arc::new(FakeUsbProblems::default()))
    }

    fn machine_with(
        dir: &Path,
        discovery: Arc<FakeDiscovery>,
        usb_problems: Arc<FakeUsbProblems>,
    ) -> Arc<Machine> {
        machine_on(dir, discovery, usb_problems, Arc::new(MonotonicClock::new()))
    }

    fn machine_on(
        dir: &Path,
        discovery: Arc<FakeDiscovery>,
        usb_problems: Arc<FakeUsbProblems>,
        clock: SharedClock,
    ) -> Arc<Machine> {
        let replays = ReplayLibrary::with_demo();
        Machine::start(MachineOptions {
            data_dir: dir.to_path_buf(),
            clock,
            discovery,
            usb_problems,
            disks: Arc::new(FakeDisks::default()),
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: crate::connectors::built_in(),
            timing: Timing {
                backoff_start: Duration::from_millis(10),
                port_retry_max: Duration::from_millis(50),
                ..Timing::default()
            },
            port_scan: None,
            world: Arc::new(simparts::SimWorld::new()),
        })
        .unwrap()
    }

    fn no_ports() -> Arc<FakeDiscovery> {
        Arc::new(FakeDiscovery(Mutex::new(vec![])))
    }

    #[test]
    fn plugging_in_an_adapter_shows_up_without_being_asked() {
        let dir = tempfile::tempdir().unwrap();
        let discovery = no_ports();
        let replays = ReplayLibrary::with_demo();
        let m = Machine::start(MachineOptions {
            data_dir: dir.path().to_path_buf(),
            clock: Arc::new(MonotonicClock::new()),
            discovery: Arc::clone(&discovery) as Arc<dyn SerialDiscovery>,
            usb_problems: Arc::new(FakeUsbProblems::default()),
            disks: Arc::new(FakeDisks::default()),
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: vec![ldrobot_ld19::definition()],
            timing: Timing::default(),
            port_scan: Some(Duration::from_millis(50)),
            world: Arc::new(simparts::SimWorld::new()),
        })
        .unwrap();
        let mut changes = m.ports_changed();
        changes.borrow_and_update();
        discovery.0.lock().unwrap().push(cp2102());
        let start = Instant::now();
        while !changes.has_changed().unwrap() {
            assert!(start.elapsed() < Duration::from_secs(2), "the scanner never noticed the new port");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(m.ports().iter().any(|p| p.entry.id.to_string() == "usb:10c4:ea60"));
        discovery.0.lock().unwrap().clear();
        changes.borrow_and_update();
        let start = Instant::now();
        while !changes.has_changed().unwrap() {
            assert!(start.elapsed() < Duration::from_secs(2), "the scanner never noticed the port leave");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(m.ports().iter().all(|p| p.entry.id.to_string() != "usb:10c4:ea60"));
        m.shutdown();
    }

    #[test]
    fn a_usb_problem_appearing_or_going_away_is_a_ports_change() {
        let dir = tempfile::tempdir().unwrap();
        let usb = Arc::new(FakeUsbProblems::default());
        let m = machine_with(dir.path(), no_ports(), Arc::clone(&usb));
        let mut changes = m.ports_changed();
        changes.borrow_and_update();
        assert!(m.port_problems().is_empty());

        usb.0.lock().unwrap().push(reset_failed(r"USB\VID_0000&PID_0001\5&1"));
        m.refresh_ports();
        assert!(changes.has_changed().unwrap(), "a new problem must re-send the ports topic");
        assert_eq!(m.port_problems().len(), 1);
        assert_eq!(m.port_problems()[0].code, "usb-no-answer");

        changes.borrow_and_update();
        m.refresh_ports();
        assert!(!changes.has_changed().unwrap(), "the same problem again is not a change");

        usb.0.lock().unwrap().clear();
        m.refresh_ports();
        assert!(changes.has_changed().unwrap(), "a problem going away must re-send the ports topic");
        assert!(m.port_problems().is_empty());
        m.shutdown();
    }

    #[test]
    fn the_order_the_os_lists_problems_in_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let usb = Arc::new(FakeUsbProblems::default());
        let m = machine_with(dir.path(), no_ports(), Arc::clone(&usb));
        let a = reset_failed(r"USB\VID_0000&PID_0001\5&A");
        let b = FailedUsb {
            os_id: r"USB\VID_1A86&PID_7523\5&B".into(),
            hardware_ids: vec![r"USB\VID_1A86&PID_7523".into()],
            problem_code: Some(28),
        };
        *usb.0.lock().unwrap() = vec![a.clone(), b.clone()];
        m.refresh_ports();
        let mut changes = m.ports_changed();
        changes.borrow_and_update();
        *usb.0.lock().unwrap() = vec![b, a];
        m.refresh_ports();
        assert!(!changes.has_changed().unwrap());
        assert_eq!(m.port_problems().len(), 2);
        m.shutdown();
    }

    #[test]
    fn things_disabled_on_purpose_are_not_problems() {
        let dir = tempfile::tempdir().unwrap();
        let usb = Arc::new(FakeUsbProblems::default());
        let m = machine_with(dir.path(), no_ports(), Arc::clone(&usb));
        let mut changes = m.ports_changed();
        changes.borrow_and_update();
        let disabled = FailedUsb {
            os_id: r"USB\VID_10C4&PID_EA60\0001".into(),
            hardware_ids: vec![],
            problem_code: Some(22),
        };
        usb.0.lock().unwrap().push(disabled);
        m.refresh_ports();
        assert!(!changes.has_changed().unwrap());
        assert!(m.port_problems().is_empty());
        m.shutdown();
    }

    fn demo(name: &str) -> NewPart {
        NewPart {
            name: name.into(),
            connector: "ldrobot-ld19".into(),
            port: Some("replay:demo-ld19".into()),
            simulate: None,
            mount: Mount::default(),
            settings: Settings::new(),
        }
    }

    fn wait_for_status(m: &Machine, name: &str, f: impl Fn(&PartStatus) -> bool) -> PartStatus {
        let start = Instant::now();
        loop {
            let s = m.parts().into_iter().find(|p| p.name.as_str() == name).map(|p| p.status);
            if let Some(s) = &s
                && f(s)
            {
                return s.clone();
            }
            assert!(start.elapsed() < Duration::from_secs(5), "timed out; status {s:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn first_run_creates_parts_toml_with_a_header() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let text = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
        assert!(text.starts_with("# The parts on this Faber machine."));
        assert!(m.parts().is_empty());
        m.shutdown();
    }

    #[test]
    fn the_demo_recording_is_always_a_port_with_a_suggestion() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let ports = m.ports();
        let demo = ports.iter().find(|p| p.entry.id.to_string() == "replay:demo-ld19").unwrap();
        assert_eq!(demo.suggestions[0].connector, "ldrobot-ld19");
        assert_eq!(demo.claimed_by, None);
        m.shutdown();
    }

    #[test]
    fn adding_a_part_writes_the_file_runs_it_and_claims_its_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let view = m.add_part(demo("lidar")).unwrap();
        assert_eq!(view.signals[0].address.to_string(), "lidar/laser-scan");
        let text = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
        assert!(text.contains("[parts.lidar]") && text.contains("replay:demo-ld19"), "{text}");
        assert!(text.starts_with("# The parts"), "the header survives");
        wait_for_status(&m, "lidar", |s| *s == PartStatus::Running);
        let port = m.ports().into_iter().find(|p| p.entry.id == PortId::Replay(DEMO_LD19.into())).unwrap();
        assert_eq!(port.claimed_by.as_deref(), Some("lidar"));
        m.shutdown();
    }

    #[test]
    fn parts_come_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_part(demo("lidar")).unwrap();
        m.shutdown();
        drop(m);
        let m = machine(dir.path(), no_ports());
        wait_for_status(&m, "lidar", |s| *s == PartStatus::Running);
        m.shutdown();
    }

    #[test]
    fn add_part_refuses_with_actionable_errors() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_part(demo("lidar")).unwrap();
        let cases = [
            (demo("Bad Name"), "invalid-name"),
            (demo("lidar"), "name-taken"),
            (NewPart { connector: "nope".into(), ..demo("a") }, "unknown-connector"),
            (NewPart { port: Some("serial:COM99".into()), ..demo("b") }, "unknown-port"),
            (NewPart { simulate: Some(Simulate::Manual), ..demo("c") }, "port-and-simulate"),
            (demo("second"), "port-claimed"),
        ];
        for (req, code) in cases {
            let e = m.add_part(req).unwrap_err();
            assert_eq!(e.code, code);
            assert!(!e.hint.is_empty(), "{code} has a hint");
        }
        let claimed = m.add_part(demo("second")).unwrap_err();
        assert!(claimed.message.contains("used by the part \"lidar\""), "{claimed:?}");
        m.shutdown();
    }

    #[test]
    fn a_simulated_part_is_written_with_simulate_and_claims_no_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let view =
            m.add_part(NewPart { port: None, simulate: Some(Simulate::Manual), ..demo("lidar") }).unwrap();
        assert_eq!(view.source, Source::Simulated(Simulate::Manual));
        let text = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
        assert!(text.contains("simulate = \"manual\"") && !text.contains("replay:demo-ld19"), "{text}");
        assert!(m.ports().iter().all(|p| p.claimed_by.is_none()), "a simulated part claims no port");
        m.shutdown();
    }

    #[test]
    fn a_generic_part_without_simulate_is_refused_with_a_hint() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let e = m.add_part(NewPart { connector: "generic-planar-base".into(), ..demo("base") }).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::BadRequest, "needs-bridge"));
        assert!(e.message.contains("FaberLink board") && e.hint.contains("Simulated"), "{e:?}");
        m.shutdown();
    }

    #[test]
    fn a_generic_part_on_a_port_in_parts_toml_is_invalid_and_claims_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(
            dir.path().join("config/parts.toml"),
            "[parts.base]\nconnector = \"generic-planar-base\"\nport = \"replay:demo-ld19\"\n",
        )
        .unwrap();
        let m = machine(dir.path(), no_ports());
        let PartStatus::Invalid(p) = wait_for_status(&m, "base", |s| matches!(s, PartStatus::Invalid(_)))
        else {
            unreachable!()
        };
        assert!(p.message.contains("through a FaberLink board"), "{p:?}");
        assert!(m.ports().iter().all(|p| p.claimed_by.is_none()));
        m.shutdown();
    }

    #[test]
    fn generic_settings_are_checked_when_the_part_is_added() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let mut settings = Settings::new();
        settings.insert("max-vx".into(), (-1.0).into());
        let req = NewPart {
            connector: "generic-planar-base".into(),
            port: None,
            simulate: Some(Simulate::Manual),
            settings,
            ..demo("base")
        };
        let e = m.add_part(req).unwrap_err();
        assert_eq!(e.code, "invalid-settings");
        assert!(e.message.contains("max-vx"), "{e:?}");
        m.shutdown();
    }

    #[test]
    fn removing_a_part_stops_it_and_frees_its_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_part(demo("lidar")).unwrap();
        m.remove_part("lidar").unwrap();
        assert!(m.parts().is_empty());
        assert!(m.router().subscribe(&SignalAddress::parse("lidar/laser-scan").unwrap()).is_none());
        assert!(m.ports().iter().all(|p| p.claimed_by.is_none()));
        assert_eq!(m.remove_part("lidar").unwrap_err().code, "unknown-part");
        m.shutdown();
    }

    #[test]
    fn a_part_waits_for_its_port_and_starts_when_it_appears() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(
            dir.path().join("config/parts.toml"),
            "[parts.lidar]\nconnector = \"ldrobot-ld19\"\nport = \"usb:10c4:ea60\"\n",
        )
        .unwrap();
        let discovery = no_ports();
        let m = machine(dir.path(), Arc::clone(&discovery));
        let PartStatus::Conflict(p) = wait_for_status(&m, "lidar", |s| matches!(s, PartStatus::Conflict(_)))
        else {
            unreachable!()
        };
        assert!(p.message.contains("not connected"));
        discovery.0.lock().unwrap().push(cp2102());
        m.refresh_ports();
        // The fake port can't really be opened, so the part moves on to Failed (and keeps retrying).
        let PartStatus::Failed(p) = wait_for_status(&m, "lidar", |s| matches!(s, PartStatus::Failed(_)))
        else {
            unreachable!()
        };
        assert!(p.message.contains("Could not open"), "{p:?}");
        m.shutdown();
    }

    #[test]
    fn a_broken_parts_toml_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let broken = "[parts.lidar]\nconnector = \n";
        std::fs::write(dir.path().join("config/parts.toml"), broken).unwrap();
        let m = machine(dir.path(), no_ports());
        assert_eq!(m.config_problem().unwrap().line, Some(2));
        let e = m.add_part(demo("x")).unwrap_err();
        assert_eq!(e.code, "config-broken");
        assert!(e.message.contains("line 2"));
        assert_eq!(std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap(), broken);
        m.shutdown();
    }

    #[test]
    fn editing_parts_toml_in_files_applies_it_or_explains_the_mistake() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let good = "# mine
[parts.lidar]
connector = \"ldrobot-ld19\"
port = \"replay:demo-ld19\"
";
        m.write_file("home:/System/parts.toml", good.as_bytes(), WriteMode::Replace).unwrap();
        wait_for_status(&m, "lidar", |s| *s == PartStatus::Running);
        let bad = "[parts.lidar]
connector = 
";
        let e = m.write_file("home:/System/parts.toml", bad.as_bytes(), WriteMode::Replace).unwrap_err();
        assert_eq!((e.code, e.kind), ("config-invalid", ErrorKind::Unprocessable));
        assert!(e.message.contains("line 2"), "{e:?}");
        let (_, on_disk) = m.read_file("home:/System/parts.toml").unwrap();
        assert_eq!(on_disk, good.as_bytes(), "a broken config is never saved");
        m.shutdown();
    }

    #[test]
    fn the_sample_recording_becomes_a_replay_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        assert!(m.ports().iter().all(|p| p.entry.id.to_string() != "replay:ld19-sample"));
        assert_eq!(m.add_samples().unwrap(), "home:/Recordings/ld19-sample.fbcap");
        let port = m.ports().into_iter().find(|p| p.entry.id.to_string() == "replay:ld19-sample").unwrap();
        assert_eq!(port.suggestions[0].connector, "ldrobot-ld19");
        assert_eq!(m.add_samples().unwrap_err().code, "already-exists");
        m.trash_files(&["home:/Recordings/ld19-sample.fbcap".into()]).unwrap();
        assert!(
            m.ports().iter().all(|p| p.entry.id.to_string() != "replay:ld19-sample"),
            "trashing removes the port"
        );
        m.shutdown();
    }

    #[test]
    fn recording_a_port_creates_a_new_replay_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let (tx, rx) = std::sync::mpsc::channel();
        let mut session = m.open_serial("replay:demo-ld19", 230_400).unwrap();
        assert_eq!(m.start_recording(&session, "bench").unwrap(), "home:/Recordings/bench.fbcap");
        assert_eq!(m.start_recording(&session, "again").unwrap_err().code, "already-recording");
        session.start(move |e| tx.send(e).is_ok());
        for _ in 0..3 {
            assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), PortEvent::Data { .. }));
        }
        let (path, bytes) = m.stop_recording(&session).unwrap();
        assert_eq!(path, "home:/Recordings/bench.fbcap");
        assert!(bytes > 0);
        assert!(m.ports().iter().any(|p| p.entry.id.to_string() == "replay:bench"));
        assert_eq!(m.start_recording(&session, "Bad Name").unwrap_err().code, "invalid-name");
        drop(session);
        m.shutdown();
    }

    #[test]
    fn file_changes_are_announced_per_folder() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let mut rx = m.files_changed();
        m.create_folder("home:/Documents/projects").unwrap();
        assert_eq!(rx.try_recv().unwrap(), ["home:/Documents"]);
        m.write_file("home:/Desktop/todo.txt", b"solder", WriteMode::Replace).unwrap();
        assert_eq!(rx.try_recv().unwrap(), ["home:/Desktop"]);
        m.add_part(demo("lidar")).unwrap();
        assert_eq!(rx.try_recv().unwrap(), ["home:/System"], "adding a part changes parts.toml");
        m.shutdown();
    }

    fn read_text(m: &Machine, path: &str) -> String {
        String::from_utf8(m.read_file(path).unwrap().1).unwrap()
    }

    fn trash_names(m: &Machine) -> Vec<String> {
        m.list_files("home:/Trash").unwrap().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn a_backup_brings_back_parts_settings_and_files_and_keeps_what_it_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_part(demo("lidar")).unwrap();
        m.write_file("home:/Documents/notes.txt", b"v1", WriteMode::Replace).unwrap();
        let mut s = m.settings();
        s.appearance.theme = crate::settings::Theme::Dark;
        m.update_settings(s).unwrap();

        let (path, bytes) = m.create_backup(false).unwrap();
        assert!(path.starts_with("home:/Backups/faber-backup-") && path.ends_with(".zip"), "{path}");
        assert!(bytes > 0);
        let listed = m.list_files("home:/Backups").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path.to_string(), path);

        m.remove_part("lidar").unwrap();
        m.write_file("home:/Documents/notes.txt", b"v2", WriteMode::Replace).unwrap();
        m.write_file("home:/Documents/new.txt", b"keep me", WriteMode::Replace).unwrap();
        m.reset_settings().unwrap();
        let mut files = m.files_changed();

        m.restore_backup(&path).unwrap();
        assert_eq!(m.parts().len(), 1, "parts.toml is back and applied");
        assert_eq!(m.settings().appearance.theme, crate::settings::Theme::Dark);
        assert_eq!(read_text(&m, "home:/Documents/notes.txt"), "v1");
        assert_eq!(read_text(&m, "home:/Documents/new.txt"), "keep me", "files not in the backup stay");
        assert_eq!(trash_names(&m), ["notes.txt"], "the replaced version is in the Trash");
        let mut announced = Vec::new();
        while let Ok(f) = files.try_recv() {
            announced.extend(f);
        }
        assert!(announced.contains(&"home:/".to_string()), "{announced:?}");

        m.restore_backup(&path).unwrap();
        assert_eq!(trash_names(&m), ["notes.txt"], "files that are the same aren't moved again");
        m.shutdown();
    }

    #[test]
    fn a_backup_with_a_broken_parts_toml_changes_nothing() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let zip_path = dir.path().join("files/Backups/broken.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("config/parts.toml", o).unwrap();
        w.write_all(b"[parts.lidar]\nconnector = \n").unwrap();
        w.start_file("files/Documents/x.txt", o).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        let before = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();

        let e = m.restore_backup("home:/Backups/broken.zip").unwrap_err();
        assert_eq!((e.code, e.kind), ("config-invalid", ErrorKind::Unprocessable));
        assert!(e.message.contains("line 2"), "{e:?}");
        assert!(m.read_file("home:/Documents/x.txt").is_err(), "no file was unpacked");
        assert_eq!(std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap(), before);
        m.shutdown();
    }

    #[test]
    fn restore_needs_a_backup_in_the_home_folder() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        assert_eq!(m.restore_backup("machine:/C:/x.zip").unwrap_err().code, "bad-path");
        assert_eq!(m.restore_backup("home:/Backups/missing.zip").unwrap_err().code, "not-found");
        m.write_file("home:/Documents/a.zip", b"not a zip", WriteMode::Replace).unwrap();
        let e = m.restore_backup("home:/Documents/a.zip").unwrap_err();
        assert_eq!((e.code, e.kind), ("backup-invalid", ErrorKind::Unprocessable));
        m.shutdown();
    }

    #[test]
    fn recordings_go_into_a_backup_only_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_samples().unwrap();
        let (without, _) = m.create_backup(false).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let (with, _) = m.create_backup(true).unwrap();
        assert_ne!(without, with, "two backups in one minute get different names");
        m.trash_files(&["home:/Recordings/ld19-sample.fbcap".into()]).unwrap();
        m.empty_trash().unwrap();
        m.restore_backup(&without).unwrap();
        assert!(m.read_file("home:/Recordings/ld19-sample.fbcap").is_err());
        m.restore_backup(&with).unwrap();
        assert!(m.read_file("home:/Recordings/ld19-sample.fbcap").is_ok());
        assert!(
            m.ports().iter().any(|p| p.entry.id.to_string() == "replay:ld19-sample"),
            "a restored recording is a replay port at once"
        );
        m.shutdown();
    }

    #[test]
    fn storage_counts_each_folder_and_the_free_space() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.write_file("home:/Documents/a.txt", b"12345", WriteMode::Replace).unwrap();
        m.write_file("home:/Desktop/b.txt", b"123", WriteMode::Replace).unwrap();
        m.add_samples().unwrap();
        let s = m.storage().unwrap();
        assert_eq!((s.documents, s.desktop, s.backups), (5, 3, 0));
        assert_eq!(s.recordings, SAMPLE_LD19_BYTES.len() as u64);
        assert!(s.total > 0 && s.free <= s.total);
        m.shutdown();
    }

    #[test]
    fn about_says_where_the_data_is_and_which_time_zone() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let a = m.about();
        assert_eq!(a.data_dir, dir.path());
        assert!(!a.os.is_empty());
        assert_eq!(a.time_zone, m.time_zone());
        assert!(!a.time_zone.is_empty());
        m.shutdown();
    }

    #[test]
    fn about_shows_a_relative_data_folder_as_a_whole_path() {
        let relative = Path::new("target").join("about-relative-test");
        let m = machine(&relative, no_ports());
        let shown = m.about().data_dir;
        m.shutdown();
        let _ = std::fs::remove_dir_all(&relative);
        assert!(shown.is_absolute(), "{}", shown.display());
        assert!(shown.ends_with(&relative));
    }

    #[test]
    fn resetting_settings_leaves_parts_and_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        m.add_part(demo("lidar")).unwrap();
        m.write_file("home:/Documents/a.txt", b"mine", WriteMode::Replace).unwrap();
        m.change_developer(crate::settings::DeveloperChange {
            full_machine_files: true,
            acknowledged_risk: true,
        })
        .unwrap();
        assert_eq!(m.reset_settings().unwrap(), MachineSettings::default());
        assert!(!m.settings().developer.full_machine_files);
        assert_eq!(m.parts().len(), 1);
        assert_eq!(read_text(&m, "home:/Documents/a.txt"), "mine");
        m.shutdown();
    }

    #[test]
    fn the_whole_machine_needs_the_developer_setting() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        assert_eq!(m.file_roots().len(), 1);
        let e = m.list_files("machine:/").unwrap_err();
        assert_eq!((e.code, e.kind), ("machine-files-off", ErrorKind::Forbidden));
        m.change_developer(crate::settings::DeveloperChange {
            full_machine_files: true,
            acknowledged_risk: true,
        })
        .unwrap();
        assert_eq!(m.file_roots().len(), 2);
        assert!(m.file_roots()[1].2.is_some(), "the machine root carries a warning");
        assert!(m.list_files("machine:/").is_ok());
        m.shutdown();
    }

    #[test]
    fn the_serial_monitor_and_parts_never_share_a_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let (tx, rx) = std::sync::mpsc::channel();
        let mut session = m.open_serial("replay:demo-ld19", 230_400).unwrap();
        assert_eq!(session.port(), "replay:demo-ld19");
        session.start(move |e| tx.send(e).is_ok());
        assert!(matches!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), PortEvent::Data { .. }));
        let e = m.add_part(demo("lidar")).unwrap_err();
        assert_eq!(e.code, "port-claimed");
        assert!(e.message.contains("serial monitor"), "{e:?}");
        drop(session);
        m.add_part(demo("lidar")).unwrap();
        let e = m.open_serial("replay:demo-ld19", 230_400).err().unwrap();
        assert_eq!(e.code, "port-claimed");
        m.shutdown();
    }

    #[test]
    fn a_plugged_in_disk_becomes_a_place_in_files_and_goes_away_again() {
        let dir = tempfile::tempdir().unwrap();
        let stick = tempfile::tempdir().unwrap();
        std::fs::write(stick.path().join("scan.fbcap"), b"x").unwrap();
        let disks = Arc::new(FakeDisks::default());
        let replays = ReplayLibrary::with_demo();
        let m = Machine::start(MachineOptions {
            data_dir: dir.path().to_path_buf(),
            clock: Arc::new(MonotonicClock::new()),
            discovery: no_ports(),
            usb_problems: Arc::new(FakeUsbProblems::default()),
            disks: Arc::clone(&disks) as Arc<dyn DiskDiscovery>,
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: vec![ldrobot_ld19::definition()],
            timing: Timing::default(),
            port_scan: None,
            world: Arc::new(simparts::SimWorld::new()),
        })
        .unwrap();
        let mut changes = m.files_changed();
        disks.0.lock().unwrap().push(Disk {
            id: "disk-e".into(),
            label: "SENNEP SD (E:)".into(),
            mount: stick.path().to_path_buf(),
        });
        m.refresh_ports();
        assert_eq!(changes.try_recv().unwrap(), vec!["disk-e:/".to_string()]);
        let roots = m.file_roots();
        assert_eq!((roots[1].0.to_string(), roots[1].1.as_str()), ("disk-e:/".into(), "SENNEP SD (E:)"));
        assert_eq!(m.list_files("disk-e:/").unwrap()[0].name, "scan.fbcap");
        m.refresh_ports();
        assert!(changes.try_recv().is_err(), "nothing changed, so nothing is announced");
        disks.0.lock().unwrap().clear();
        m.refresh_ports();
        assert_eq!(changes.try_recv().unwrap(), vec!["disk-e:/".to_string()]);
        assert_eq!(m.file_roots().len(), 1);
        assert_eq!(m.list_files("disk-e:/").unwrap_err().code, "not-found");
        m.shutdown();
    }

    // ---- leases, commands and the e-stop -------------------------------------------------

    use crate::router::Subscription;
    use crate::sim::world::Pose;
    use crate::simulation::Pattern;
    use faber_capabilities::{Command, Twist2d, planar_base};

    fn sim(m: &Machine, name: &str, connector: &str, mode: Simulate, settings: &str) -> PartView {
        m.add_part(NewPart {
            name: name.into(),
            connector: connector.into(),
            port: None,
            simulate: Some(mode),
            mount: Mount::default(),
            settings: toml::from_str(settings).unwrap(),
        })
        .unwrap()
    }

    fn drive(vx: f32, omega: f32) -> Command {
        Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(vx, 0.0, omega)))
    }

    fn latest(m: &Machine, addr: &str) -> Option<Reading> {
        match m.router().subscribe(&SignalAddress::parse(addr).unwrap())? {
            Subscription::Latest(rx) => rx.borrow().as_deref().cloned(),
            Subscription::Every(_) => None,
        }
    }

    fn commanded(m: &Machine) -> Twist2d {
        match latest(m, "base/planar-base") {
            Some(Reading::PlanarBase(r)) => r.commanded,
            other => panic!("no base reading: {other:?}"),
        }
    }

    fn base_machine(dir: &Path) -> Arc<Machine> {
        let m = machine(dir, no_ports());
        sim(&m, "base", "generic-planar-base", Simulate::Manual, "max-vx = 0.5\nmax-omega = 1.0\n");
        wait_for_status(&m, "base", |s| *s == PartStatus::Running);
        m
    }

    #[test]
    fn a_part_shows_the_limits_its_own_settings_give() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let added = sim(
            &m,
            "base",
            "generic-planar-base",
            Simulate::Manual,
            "max-vx = 0.6
",
        );
        let listed = m.parts().into_iter().find(|p| p.name.as_str() == "base").unwrap();
        for view in [added, listed] {
            let Some(Info::PlanarBase(info)) = &view.signals[0].info else { panic!("{view:?}") };
            assert_eq!(info.max_velocity.vx, 0.6, "not the connector's default 0.5");
        }
        m.shutdown();
    }

    #[test]
    fn a_leased_command_is_clamped_applied_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let lease = m.request_lease("base", "test", false).unwrap();
        let reply = m.command("base", &lease.id, drive(2.0, 0.3)).unwrap();
        assert_eq!(reply.applied, drive(0.5, 0.3));
        assert!(reply.clamped);
        assert!(reply.deadline_us >= lease.deadline_us, "a command renews the lease");
        assert_eq!(commanded(&m), Twist2d::new(0.5, 0.0, 0.3), "the base reports it at once");
        let e = m.request_lease("base", "someone else", false).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::Conflict, "lease-held"));
        assert!(e.message.contains("\"test\""), "{e:?}");
        m.shutdown();
    }

    #[test]
    fn commands_need_a_live_lease() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let e = m.command("base", "nope", drive(0.1, 0.0)).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::Forbidden, "lease-required"));
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.2, 0.0)).unwrap();
        m.release_lease("base", &lease.id).unwrap();
        assert_eq!(commanded(&m), Twist2d::ZERO, "releasing stops the base");
        let e = m.command("base", &lease.id, drive(0.1, 0.0)).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::Forbidden, "lease-ended"));
        assert!(e.message.contains("released"), "{e:?}");
        assert_eq!(e.lease_ended, Some(EndReason::Released), "the reason comes typed too");
        assert_eq!(m.command("ghost", &lease.id, drive(0.1, 0.0)).unwrap_err().code, "unknown-part");
        m.shutdown();
    }

    #[test]
    fn a_lease_that_goes_quiet_expires_and_stops_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let mut events = m.lease_events();
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.3, 0.0)).unwrap();
        let sent = Instant::now();
        // The exact bound (deadline + one sweep) is checked on a hand-moved clock below; here
        // the real sweeper thread does it, so allow for a busy test machine.
        while commanded(&m) != Twist2d::ZERO {
            assert!(sent.elapsed() < Duration::from_secs(2), "the sweeper stops the base");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(sent.elapsed() >= Duration::from_millis(240), "not before the lease ran out");
        let e = m.command("base", &lease.id, drive(0.1, 0.0)).unwrap_err();
        assert!(e.code == "lease-ended" && e.message.contains("expired"), "{e:?}");
        assert!(matches!(events.try_recv(), Ok(LeaseEvent::Granted(_))));
        let Ok(LeaseEvent::Ended(ended)) = events.try_recv() else { panic!("lease-ended is announced") };
        assert_eq!(ended.reason, EndReason::Expired);
        m.shutdown();
    }

    #[test]
    fn the_sweeper_ends_a_lease_within_50_ms_of_its_deadline() {
        assert!(safety::SWEEP_EVERY <= Duration::from_millis(25), "a sweep at least twice per 50 ms");
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(crate::clock::ManualClock::new(1_000_000));
        let m = machine_on(dir.path(), no_ports(), Arc::new(FakeUsbProblems::default()), clock.clone());
        sim(&m, "base", "generic-planar-base", Simulate::Manual, "");
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.3, 0.0)).unwrap();
        clock.set(lease.deadline_us - 1);
        m.sweep();
        assert_ne!(commanded(&m), Twist2d::ZERO, "still leased just before the deadline");
        clock.set(lease.deadline_us);
        m.sweep();
        assert_eq!(commanded(&m), Twist2d::ZERO, "the first sweep after the deadline stops it");
        m.shutdown();
    }

    #[test]
    fn the_estop_stops_everything_and_refuses_leases_until_released() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let mut changes = m.estop_events();
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.3, 0.0)).unwrap();
        let view = m.engage_estop(estop::Source::Shell, "Stop button");
        assert_eq!(view.engaged.unwrap().reason, "Stop button");
        assert_eq!(commanded(&m), Twist2d::ZERO);
        assert!(m.leases().is_empty());
        let e = m.command("base", &lease.id, drive(0.1, 0.0)).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::Locked, "estop-engaged"));
        assert_eq!(m.request_lease("base", "test", false).unwrap_err().code, "estop-engaged");
        m.engage_estop(estop::Source::App, "again");
        assert_eq!(m.estop().engaged.unwrap().reason, "Stop button", "the first reason stays");
        assert!(m.release_estop(estop::Source::Shell).unwrap().engaged.is_none());
        assert!(m.request_lease("base", "test", false).is_ok());
        let seen: Vec<bool> = std::iter::from_fn(|| changes.try_recv().ok()).map(|r| r.engaged).collect();
        assert_eq!(seen, [true, false], "every change is announced once");
        assert_eq!(m.estop().history.len(), 2);
        m.shutdown();
    }

    #[test]
    fn an_operator_takes_over_and_the_app_hears_it() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let app = m.request_lease("base", "drive-square.py", false).unwrap();
        m.command("base", &app.id, drive(0.3, 0.0)).unwrap();
        let op = m.request_lease("base", "Drive window", true).unwrap();
        assert_eq!(commanded(&m), Twist2d::ZERO, "the safe state is sent when the old lease ends");
        let e = m.command("base", &app.id, drive(0.3, 0.0)).unwrap_err();
        assert!(e.message.contains("took over"), "{e:?}");
        assert!(m.command("base", &op.id, drive(0.1, 0.0)).is_ok());
        m.shutdown();
    }

    #[test]
    fn removing_or_restarting_a_part_ends_its_lease() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        let lease = m.request_lease("base", "test", false).unwrap();
        m.remove_part("base").unwrap();
        assert!(m.leases().is_empty());
        sim(&m, "base", "generic-planar-base", Simulate::Manual, "");
        let e = m.command("base", &lease.id, drive(0.1, 0.0)).unwrap_err();
        assert!(e.message.contains("removed"), "{e:?}");
        m.shutdown();
    }

    #[test]
    fn a_base_lease_locks_the_motors_it_is_built_from() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        sim(&m, "left", "generic-motor", Simulate::Manual, "");
        sim(&m, "base", "generic-planar-base", Simulate::Manual, "built-from = [\"left\"]\n");
        let base = m.request_lease("base", "Drive window", false).unwrap();
        assert_eq!(base.locks, [PartName::parse("left").unwrap()]);
        let e = m.request_lease("left", "motor test", false).unwrap_err();
        assert_eq!(e.code, "lease-held");
        assert!(e.message.contains("part of \"base\""), "{e:?}");
        let e = m
            .command("left", &base.id, Command::Motor(faber_capabilities::motor::Command::STOP))
            .unwrap_err();
        assert_eq!(e.code, "lease-required");
        assert!(e.message.contains("part of \"base\""), "{e:?}");
        m.shutdown();
    }

    #[test]
    fn commands_and_leases_only_fit_what_a_part_is() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        sim(&m, "front", "generic-distance", Simulate::Manual, "");
        assert_eq!(m.request_lease("front", "test", false).unwrap_err().code, "not-leasable");
        assert_eq!(m.request_lease("base", "  ", false).unwrap_err().code, "bad-request");
        let lease = m.request_lease("base", "test", false).unwrap();
        let motor = Command::Motor(faber_capabilities::motor::Command::STOP);
        let e = m.command("base", &lease.id, motor).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::BadRequest, "bad-command"));
        assert!(e.hint.contains("planar-base@1"), "{e:?}");
        assert_eq!(m.command("base", &lease.id, drive(f32::NAN, 0.0)).unwrap_err().code, "bad-command");
        m.shutdown();
    }

    #[test]
    fn manual_sensors_take_a_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let m = base_machine(dir.path());
        sim(&m, "front", "generic-distance", Simulate::Manual, "max-m = 2.0\nrate-hz = 100\n");
        m.set_pattern("front", Pattern::Steady { value: 0.75 }).unwrap();
        assert_eq!(m.pattern("front").unwrap(), Pattern::Steady { value: 0.75 });
        let start = Instant::now();
        while !matches!(latest(&m, "front/distance"), Some(Reading::Distance(r)) if r.meters == 0.75) {
            assert!(start.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(m.set_pattern("base", Pattern::Steady { value: 1.0 }).unwrap_err().code, "not-a-sensor");
        let bad = Pattern::Sine { center: 1.0, amplitude: 1.0, period_s: 0.0 };
        assert_eq!(m.set_pattern("front", bad).unwrap_err().code, "bad-pattern");
        m.add_part(demo("lidar")).unwrap();
        assert_eq!(m.set_pattern("lidar", Pattern::Steady { value: 1.0 }).unwrap_err().code, "not-manual");
        m.shutdown();
    }

    #[test]
    fn world_parts_run_in_the_world_and_only_one_base_is_the_machine() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        sim(&m, "base", "generic-planar-base", Simulate::World, "length = 0.8\nwidth = 0.65\n");
        wait_for_status(&m, "base", |s| *s == PartStatus::Running);
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.3, 0.0)).unwrap();
        assert_eq!(commanded(&m), Twist2d::new(0.3, 0.0, 0.0));
        let start = Instant::now();
        while m.world_state().velocity.vx <= 0.0 {
            assert!(start.elapsed() < Duration::from_secs(2), "the world base never moved");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(latest(&m, "world/pose").is_some(), "the ground truth is published");
        assert_eq!(m.world().footprint.map(|f| (f.length, f.width)), Some((0.8, 0.65)));
        let e = m
            .add_part(NewPart {
                connector: "generic-planar-base".into(),
                port: None,
                simulate: Some(Simulate::World),
                ..demo("second")
            })
            .unwrap_err();
        assert_eq!(e.code, "world-has-a-base");
        sim(&m, "lidar", "ldrobot-ld19", Simulate::World, "");
        wait_for_status(&m, "lidar", |s| *s == PartStatus::Running);
        m.shutdown();
    }

    #[test]
    fn without_a_world_table_world_parts_are_in_the_empty_world() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        sim(&m, "front", "generic-distance", Simulate::World, "");
        assert_eq!(m.world().world.name, DEFAULT_WORLD);
        assert_eq!(m.world_state().pose, m.world().start, "a sensor alone stands at the start");
        assert!(!read_text(&m, "home:/System/parts.toml").contains("[world]"), "nothing is written");
        let e = m
            .add_part(NewPart { simulate: Some(Simulate::Manual), port: None, ..demo("world") })
            .unwrap_err();
        assert_eq!(e.code, "invalid-name", "world/pose belongs to the world");
        m.shutdown();
    }

    #[test]
    fn an_unknown_world_is_a_config_error_with_its_line_and_the_worlds() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("parts.toml"), "# mine\n[world]\nname = \"moon\"\n").unwrap();
        let m = machine(dir.path(), no_ports());
        let p = m.config_problem().expect("a config problem");
        assert_eq!(p.line, Some(3), "{p:?}");
        assert!(p.message.contains("\"moon\"") && p.hint.contains("apartment"), "{p:?}");
        m.shutdown();

        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let e = m.write_file("home:/System/parts.toml", b"[world]\nname = \"moon\"\n", WriteMode::Replace);
        let e = e.unwrap_err();
        assert_eq!(e.code, "config-invalid", "{e:?}");
        assert!(e.message.contains("line 2"), "{e:?}");
        let e = m.set_world("moon", None).unwrap_err();
        assert_eq!((e.kind, e.code), (ErrorKind::BadRequest, "unknown-world"));
        assert!(e.hint.contains("obstacle-course"), "{e:?}");
        let e = m.set_world("apartment", Some(Pose::new(50.0, 1.0, 0.0))).unwrap_err();
        assert_eq!(e.code, "start-outside-world");
        m.shutdown();
    }

    #[test]
    fn changing_the_world_rebuilds_it_resets_the_pose_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let mut changes = m.world_events();
        sim(&m, "base", "generic-planar-base", Simulate::World, "");
        let view = m.set_world("apartment", None).unwrap();
        assert_eq!((view.world.name.as_str(), view.start), ("apartment", view.world.start));
        assert_eq!(changes.try_recv().unwrap().name, "apartment");
        assert_eq!(m.world_state().pose, view.world.start);
        let text = read_text(&m, "home:/System/parts.toml");
        assert!(text.starts_with("# The parts on this Faber machine."), "the header stays:\n{text}");
        assert!(text.contains("[world]\nname = \"apartment\""), "{text}");

        // Drive a little, then put it back.
        let lease = m.request_lease("base", "test", false).unwrap();
        m.command("base", &lease.id, drive(0.3, 0.0)).unwrap();
        let start = Instant::now();
        while m.world_state().pose == view.start {
            assert!(start.elapsed() < Duration::from_secs(2), "it never moved");
            std::thread::sleep(Duration::from_millis(5));
        }
        m.release_lease("base", &lease.id).unwrap();
        m.reset_world();
        assert_eq!(m.world_state().pose, view.start);

        // A hand edit of [world] switches it too.
        let edited =
            text.replace("name = \"apartment\"", "name = \"maze\"\nstart = { x = 1.0, y = 1.0, yaw = 0.0 }");
        m.write_file("home:/System/parts.toml", edited.as_bytes(), WriteMode::Replace).unwrap();
        assert_eq!(changes.try_recv().unwrap().name, "maze");
        assert_eq!((m.world().world.name.as_str(), m.world().start), ("maze", Pose::new(1.0, 1.0, 0.0)));
        assert_eq!(m.world_state().pose, Pose::new(1.0, 1.0, 0.0));
        // Anything else in parts.toml changing leaves the world alone.
        sim(&m, "front", "generic-distance", Simulate::Manual, "");
        assert!(changes.try_recv().is_err());
        m.shutdown();
    }

    #[test]
    fn removing_or_restarting_world_parts_leaves_no_thread_behind() {
        let dir = tempfile::tempdir().unwrap();
        let world = Arc::new(simparts::SimWorld::new());
        let replays = ReplayLibrary::with_demo();
        let m = Machine::start(MachineOptions {
            data_dir: dir.path().to_path_buf(),
            clock: Arc::new(MonotonicClock::new()),
            discovery: no_ports(),
            usb_problems: Arc::new(FakeUsbProblems::default()),
            disks: Arc::new(FakeDisks::default()),
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: crate::connectors::built_in(),
            timing: Timing::default(),
            port_scan: None,
            world: Arc::clone(&world) as Arc<dyn WorldEngine>,
        })
        .unwrap();
        sim(&m, "base", "generic-planar-base", Simulate::World, "");
        sim(&m, "lidar", "ldrobot-ld19", Simulate::World, "");
        assert_eq!(world.live_threads(), 1);
        // Restart the base by changing its settings by hand.
        let text = read_text(&m, "home:/System/parts.toml").replacen(
            "simulate = \"world\"\n",
            "simulate = \"world\"\nsettings = { max-vx = 0.4 }\n",
            1,
        );
        m.write_file("home:/System/parts.toml", text.as_bytes(), WriteMode::Replace).unwrap();
        wait_for_status(&m, "base", |s| *s == PartStatus::Running);
        assert_eq!(world.live_threads(), 1, "restarting keeps one thread");
        m.remove_part("base").unwrap();
        m.remove_part("lidar").unwrap();
        assert_eq!(world.live_threads(), 0, "removing the last world part stops it");
        assert!(
            latest(&m, "world/pose").is_none() && m.router().subscribe(&simparts::pose_address()).is_none()
        );
        sim(&m, "lidar", "ldrobot-ld19", Simulate::World, "");
        assert_eq!(world.live_threads(), 1);
        m.shutdown();
        assert_eq!(world.live_threads(), 0, "and so does shutting down");
    }
}
