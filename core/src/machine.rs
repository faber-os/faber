//! Layer 3: the machine. Ties config, ports, runtime and router together, and is the only
//! place that changes what runs. Everything it does follows one rule (D13): **`parts.toml` is
//! the source of truth**. Changes are written to the file first, then `reconcile` makes the
//! running parts match it.

use crate::clock::SharedClock;
use crate::config::{self, ConfigError, PartConfig, PartsConfig};
use crate::files::{self, Entry, FileError, Files, VPath};
use crate::names::{PartName, SignalAddress};
use crate::ports::{
    Claimant, Claims, PortEntry, PortEvent, PortId, PortOpener, PortReader, PortWriter, ReplayLibrary,
    SAMPLE_LD19_BYTES, SAMPLE_LD19_NAME, SerialDiscovery, catalog, spawn_reader,
};
use crate::router::Router;
use crate::runtime::{self, PartHandle, PartStatus, RunDeps, RunSpec, StatusBoard, Timing};
use crate::settings::{DeveloperChange, Settings as MachineSettings, SettingsError, SettingsStore};
use crate::suggest::{Suggestion, suggestions};
use faber_capabilities::{Capability, Mount};
use faber_connector::{Definition, Manifest, Problem, Settings};
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
}

impl From<FileError> for MachineError {
    fn from(e: FileError) -> Self {
        let kind = match e.code {
            "not-found" => ErrorKind::NotFound,
            "already-exists" | "protected" => ErrorKind::Conflict,
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
    pub port: String,
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

/// A request to add a part.
#[derive(Clone, Debug, PartialEq)]
pub struct NewPart {
    pub name: String,
    pub connector: String,
    pub port: String,
    pub mount: Mount,
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
    claims: Claims,
    running: BTreeMap<PartName, Running>,
}

pub struct Machine {
    clock: SharedClock,
    router: Arc<Router>,
    status: Arc<StatusBoard>,
    discovery: Arc<dyn SerialDiscovery>,
    opener: Arc<dyn PortOpener>,
    replays: ReplayLibrary,
    connectors: Vec<Definition>,
    timing: Timing,
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
            opener: opts.opener,
            replays: opts.replays,
            connectors: opts.connectors,
            timing: opts.timing,
            config_path,
            files,
            settings,
            state: Mutex::new(State {
                text,
                config,
                config_problem,
                ports: Vec::new(),
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

    /// Fires when ports appear, vanish, or are claimed or released.
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
                    port: cfg.port.clone(),
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

    /// Looks for new or removed ports, and starts parts whose port just appeared.
    pub fn refresh_ports(&self) {
        self.replays.sync_dir(&self.recordings_dir());
        let found = catalog(&self.discovery.list(), &self.replays.names());
        let mut st = self.state.lock().expect("machine lock");
        if st.ports != found {
            st.ports = found;
            self.ports_changed.send_modify(|n| *n += 1);
        }
        self.reconcile(&mut st);
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
        (definition.create)(&Settings::new()).map_err(|e| {
            MachineError::new(
                ErrorKind::BadRequest,
                "invalid-settings",
                format!("Setting \"{}\": {}", e.key, e.message),
                e.hint,
            )
        })?;
        let port = PortId::parse(&new.port)
            .and_then(|id| st.ports.iter().find(|p| p.id == id).cloned())
            .ok_or_else(|| {
                MachineError::new(
                    ErrorKind::BadRequest,
                    "unknown-port",
                    format!("There is no port called \"{}\" right now.", new.port),
                    "Check that it is plugged in, then pick it from the list of ports.",
                )
            })?;
        if let Some(holder) = st.claims.holder(&port.id) {
            return Err(port_claimed(&port, &holder.to_string()));
        }
        let cfg = PartConfig {
            connector: new.connector,
            port: port.id.to_string(),
            mount: new.mount,
            settings: Settings::new(),
        };
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
        let Some(port_id) = PortId::parse(&cfg.port) else {
            self.status.set(
                name,
                PartStatus::Invalid(Problem::new(
                    format!("\"{}\" is not a valid port.", cfg.port),
                    "Ports look like usb:10c4:ea60, serial:COM3 or replay:demo-ld19. Fix it in parts.toml.",
                )),
            );
            return None;
        };
        let Some(port) = st.ports.iter().find(|p| p.id == port_id).cloned() else {
            self.status.set(
                name,
                PartStatus::Conflict(Problem::new(
                    format!("Its port {} is not connected.", cfg.port),
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
            if let Some(id) = PortId::parse(&r.config.port) {
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

    pub fn file_roots(&self) -> Vec<(VPath, &'static str, Option<&'static str>)> {
        let mut roots = vec![(VPath::home(&[]), "Home", None)];
        if self.machine_files() {
            roots.push((
                VPath { root: files::RootKey::Machine, parts: vec![] },
                "This computer",
                Some("You're browsing the whole computer. Deleting or changing system files can break it, and there's no Trash here."),
            ));
        }
        roots
    }

    pub fn list_files(&self, path: &str) -> Result<Vec<Entry>, MachineError> {
        Ok(self.files.list(&VPath::parse(path)?, self.machine_files())?)
    }

    pub fn read_file(&self, path: &str) -> Result<(VPath, Vec<u8>), MachineError> {
        let p = VPath::parse(path)?;
        let bytes = self.files.read(&p, self.machine_files())?;
        Ok((p, bytes))
    }

    /// Writes a file. `home:/System/parts.toml` is the machine config: it is validated first,
    /// then saved and applied, exactly like adding a part in the shell.
    pub fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), MachineError> {
        let p = VPath::parse(path)?;
        if Files::is_config(&p) {
            return self.write_config_text(bytes);
        }
        self.files.write(&p, bytes, self.machine_files())?;
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
        if let Err(e) = config::parse(&text) {
            let message = match e.line {
                Some(l) => format!("parts.toml has a problem on line {l}: {}", e.message),
                None => format!("parts.toml has a problem: {}", e.message),
            };
            return Err(MachineError::new(ErrorKind::Unprocessable, "config-invalid", message, e.hint));
        }
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
        self.files.write(&p, SAMPLE_LD19_BYTES, false)?;
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
    use crate::ports::{DEMO_LD19, Discovered, SystemOpener, Usb};
    use std::time::Instant;

    struct FakeDiscovery(Mutex<Vec<Discovered>>);

    impl SerialDiscovery for FakeDiscovery {
        fn list(&self) -> Vec<Discovered> {
            self.0.lock().unwrap().clone()
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
        let replays = ReplayLibrary::with_demo();
        Machine::start(MachineOptions {
            data_dir: dir.to_path_buf(),
            clock: Arc::new(MonotonicClock::new()),
            discovery,
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: vec![ldrobot_ld19::definition()],
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

    fn demo(name: &str) -> NewPart {
        NewPart {
            name: name.into(),
            connector: "ldrobot-ld19".into(),
            port: "replay:demo-ld19".into(),
            mount: Mount::default(),
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
            (NewPart { port: "serial:COM99".into(), ..demo("b") }, "unknown-port"),
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
        m.write_file("home:/System/parts.toml", good.as_bytes()).unwrap();
        wait_for_status(&m, "lidar", |s| *s == PartStatus::Running);
        let bad = "[parts.lidar]
connector = 
";
        let e = m.write_file("home:/System/parts.toml", bad.as_bytes()).unwrap_err();
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
        m.write_file("home:/Desktop/todo.txt", b"solder").unwrap();
        assert_eq!(rx.try_recv().unwrap(), ["home:/Desktop"]);
        m.add_part(demo("lidar")).unwrap();
        assert_eq!(rx.try_recv().unwrap(), ["home:/System"], "adding a part changes parts.toml");
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
}
