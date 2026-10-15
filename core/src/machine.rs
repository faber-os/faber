//! Layer 3: the machine. Ties config, ports, runtime and router together, and is the only
//! place that changes what runs. Everything it does follows one rule (D13): **`parts.toml` is
//! the source of truth**. Changes are written to the file first, then `reconcile` makes the
//! running parts match it.

use crate::backup::{self, Archive, BackupError, Item};
use crate::clock::SharedClock;
use crate::config::{self, ConfigError, PartConfig, PartsConfig, Simulate, Source};
use crate::disks::{Disk, DiskDiscovery};
use crate::files::{self, Entry, FileError, Files, Special, VPath, WriteMode};
use crate::host;
use crate::names::{PartName, SignalAddress};
use crate::ports::{
    Claimant, Claims, PortEntry, PortEvent, PortId, PortOpener, PortProblem, PortReader, PortWriter,
    ReplayLibrary, SAMPLE_LD19_BYTES, SAMPLE_LD19_NAME, SerialDiscovery, UsbProblemDiscovery, catalog,
    port_problem, spawn_reader,
};
use crate::router::Router;
use crate::runtime::{self, PartHandle, PartStatus, RunDeps, RunSpec, StatusBoard, Timing};
use crate::settings::{
    self as settings_file, DeveloperChange, Settings as MachineSettings, SettingsError, SettingsStore,
};
use crate::suggest::{Suggestion, suggestions};
use faber_capabilities::{Capability, Mount};
use faber_connector::{Definition, Manifest, PortRequirement, Problem, Settings};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::{broadcast, watch};

const SERIAL_MONITOR: &str = "serial-monitor";
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
    pub connectors: Vec<Definition>,
    pub timing: Timing,
    /// How often to look for new or removed ports. `None` = only when asked (tests).
    pub port_scan: Option<Duration>,
}

/// Why the machine refused a request, in words the user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError {
    pub kind: ErrorKind,
    pub code: &'static str,
    pub message: String,
    pub hint: String,
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
    /// Not allowed with the current settings.
    Forbidden,
    /// The disk is too full for it.
    InsufficientStorage,
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
        MachineError { kind, code: e.code, message: e.message, hint: e.hint }
    }
}

impl From<SettingsError> for MachineError {
    fn from(e: SettingsError) -> Self {
        let kind = if e.code == "settings-not-saved" { ErrorKind::Internal } else { ErrorKind::BadRequest };
        MachineError { kind, code: e.code, message: e.message, hint: e.hint }
    }
}

impl From<BackupError> for MachineError {
    fn from(e: BackupError) -> Self {
        let kind = if e.code == "backup-invalid" { ErrorKind::Unprocessable } else { ErrorKind::Internal };
        MachineError { kind, code: e.code, message: e.message, hint: e.hint }
    }
}

impl MachineError {
    fn new(kind: ErrorKind, code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        MachineError { kind, code, message: message.into(), hint: hint.into() }
    }
}

/// A part as the shell shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PartView {
    pub name: PartName,
    pub connector: String,
    pub source: Source,
    pub mount: Mount,
    pub status: PartStatus,
    pub signals: Vec<(SignalAddress, Capability)>,
}

/// A port as the shell shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PortView {
    pub entry: PortEntry,
    pub claimed_by: Option<String>,
    pub suggestions: Vec<Suggestion>,
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
    handle: Option<PartHandle>,
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
    connectors: Vec<Definition>,
    timing: Timing,
    data_dir: PathBuf,
    config_path: PathBuf,
    files: Files,
    settings: SettingsStore,
    state: Mutex<State>,
    ports_changed: watch::Sender<u64>,
    files_changed: broadcast::Sender<Vec<String>>,
    stop_scanning: Arc<AtomicBool>,
    scanner: Mutex<Option<JoinHandle<()>>>,
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
        let (config, config_problem) = match config::parse(&text) {
            Ok(c) => (c, None),
            Err(e) => {
                tracing::error!(target: "core::machine", "parts.toml can't be read: {} {}", e.message, e.hint);
                (PartsConfig::default(), Some(e))
            }
        };
        let files = Files::open(opts.data_dir.join("files"), config_path.clone())?;
        let settings = SettingsStore::load(&config_dir);
        let machine = Arc::new(Machine {
            clock: opts.clock,
            router: Arc::new(Router::new()),
            status: Arc::new(StatusBoard::default()),
            discovery: opts.discovery,
            usb_problems: opts.usb_problems,
            disks: opts.disks,
            opener: opts.opener,
            replays: opts.replays,
            connectors: opts.connectors,
            timing: opts.timing,
            data_dir: opts.data_dir.clone(),
            config_path,
            files,
            settings,
            state: Mutex::new(State {
                text,
                config,
                config_problem,
                ports: Vec::new(),
                port_problems: Vec::new(),
                disks: Vec::new(),
                claims: Claims::default(),
                running: BTreeMap::new(),
            }),
            ports_changed: watch::channel(0).0,
            files_changed: broadcast::channel(64).0,
            stop_scanning: Arc::new(AtomicBool::new(false)),
            scanner: Mutex::new(None),
        });
        machine.refresh_ports();
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

    /// Stops every part and the port scanner.
    pub fn shutdown(&self) {
        self.stop_scanning.store(true, Ordering::Relaxed);
        if let Some(h) = self.scanner.lock().expect("scanner lock").take() {
            let _ = h.join();
        }
        let mut st = self.state.lock().expect("machine lock");
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
        self.connectors.iter().map(|d| d.manifest.clone()).collect()
    }

    fn definition(&self, id: &str) -> Option<&Definition> {
        self.connectors.iter().find(|d| d.manifest.id == id)
    }

    pub fn parts(&self) -> Vec<PartView> {
        let st = self.state.lock().expect("machine lock");
        st.config
            .parts
            .iter()
            .map(|(name, cfg)| {
                let signals = self
                    .definition(&cfg.connector)
                    .map(|d| {
                        d.manifest
                            .provides
                            .iter()
                            .map(|p| (SignalAddress::new(name.clone(), p.name.clone()), p.capability))
                            .collect()
                    })
                    .unwrap_or_default();
                PartView {
                    name: name.clone(),
                    connector: cfg.connector.clone(),
                    source: cfg.source.clone(),
                    mount: cfg.mount,
                    status: self.status.get(name).unwrap_or(PartStatus::Starting),
                    signals,
                }
            })
            .collect()
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
                PortView {
                    entry: entry.clone(),
                    claimed_by: st.claims.holder(&entry.id).map(|c| c.to_string()),
                    suggestions: suggestions(entry, recorded_with.as_deref(), &manifests),
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
            (None, Some(mode)) => Source::Simulated(mode),
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
        let cfg = PartConfig { connector: new.connector, source, mount: new.mount, settings: new.settings };
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
        let name = PartName::parse(name).map_err(|_| unknown())?;
        let mut st = self.state.lock().expect("machine lock");
        refuse_if_config_broken(&st)?;
        if !st.config.parts.contains_key(&name) {
            return Err(unknown());
        }
        let text = config::remove_part(&st.text, &name).map_err(config_write_error)?;
        self.save(&mut st, text)?;
        self.reconcile(&mut st);
        Ok(())
    }

    /// Writes the new config text, then adopts it. The file is written first, so the running
    /// machine never disagrees with what is on disk.
    fn save(&self, st: &mut State, text: String) -> Result<(), MachineError> {
        let parsed = config::parse(&text).map_err(config_write_error)?;
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
        let gone: Vec<PartName> = st
            .running
            .iter()
            .filter(|(name, r)| st.config.parts.get(*name) != Some(&r.config))
            .map(|(name, _)| name.clone())
            .collect();
        for name in gone {
            self.stop_part(st, &name);
        }
        let parts: Vec<(PartName, PartConfig)> =
            st.config.parts.iter().map(|(n, c)| (n.clone(), c.clone())).collect();
        for (name, cfg) in parts {
            let retry = st.running.get(&name).is_some_and(|r| r.handle.is_none());
            if st.running.contains_key(&name) && !retry {
                continue;
            }
            st.running.remove(&name);
            let handle = self.try_start(st, &name, &cfg);
            st.running.insert(name, Running { config: cfg, handle });
        }
    }

    fn try_start(&self, st: &mut State, name: &PartName, cfg: &PartConfig) -> Option<PartHandle> {
        let Some(definition) = self.definition(&cfg.connector) else {
            self.status.set(
                name,
                PartStatus::Invalid(Problem::new(
                    format!("There is no connector called \"{}\".", cfg.connector),
                    "Fix the connector name in parts.toml, or remove the part and add it again.",
                )),
            );
            return None;
        };
        let port_name = match &cfg.source {
            Source::Port(_) if definition.manifest.port == PortRequirement::Bridge => {
                self.status.set(name, PartStatus::Invalid(runtime::needs_bridge(&definition.manifest)));
                return None;
            }
            Source::Port(p) => p.as_str(),
            Source::Simulated(_) => {
                self.status.set(
                    name,
                    PartStatus::Invalid(Problem::new(
                        "This core can't simulate parts yet.",
                        "Update Faber, or use the real part with port = \"…\" in parts.toml.",
                    )),
                );
                return None;
            }
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
        let spec = RunSpec::new(name.clone(), definition, cfg.settings.clone(), port);
        Some(runtime::start(
            spec,
            RunDeps {
                clock: self.clock.clone(),
                opener: Arc::clone(&self.opener),
                router: Arc::clone(&self.router),
                status: Arc::clone(&self.status),
                timing: self.timing,
            },
        ))
    }

    fn stop_part(&self, st: &mut State, name: &PartName) {
        if let Some(r) = st.running.remove(name) {
            if let Some(h) = r.handle {
                h.stop();
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
        let src = backup::Sources {
            parts_toml: &self.config_path,
            settings_toml: &settings_path,
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
    /// changes, so a broken parts.toml is refused with its line and nothing is touched. Files
    /// that the backup replaces go to the Trash; files it doesn't have stay.
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
        config::parse(archive.parts_toml()).map_err(|e| config_invalid(&e))?;
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

        // 2. Then change things: the config and settings first, as they matter most.
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
        config::parse(&text).map_err(|e| config_invalid(&e))?;
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
    }
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
        let replays = ReplayLibrary::with_demo();
        Machine::start(MachineOptions {
            data_dir: dir.to_path_buf(),
            clock: Arc::new(MonotonicClock::new()),
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
        assert_eq!(view.signals[0].0.to_string(), "lidar/laser-scan");
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
        assert!(e.message.contains("Phase 4") && e.hint.contains("Simulated"), "{e:?}");
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
        assert!(p.message.contains("needs a bridge"), "{p:?}");
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
}
