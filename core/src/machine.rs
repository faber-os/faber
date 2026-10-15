//! Layer 3: the machine. Ties config, ports, runtime and router together, and is the only
//! place that changes what runs. Everything it does follows one rule (D13): **`parts.toml` is
//! the source of truth**. Changes are written to the file first, then `reconcile` makes the
//! running parts match it.

use crate::clock::SharedClock;
use crate::config::{self, ConfigError, PartConfig, PartsConfig};
use crate::names::{PartName, SignalAddress};
use crate::ports::{
    Claimant, Claims, PortEntry, PortEvent, PortId, PortOpener, PortWriter, ReplayLibrary, SerialDiscovery,
    catalog, spawn_reader,
};
use crate::router::Router;
use crate::runtime::{self, PartHandle, PartStatus, RunDeps, RunSpec, StatusBoard, Timing};
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
use tokio::sync::watch;

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
    state: Mutex<State>,
    ports_changed: watch::Sender<u64>,
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
            state: Mutex::new(State {
                text,
                config,
                config_problem,
                ports: Vec::new(),
                claims: Claims::default(),
                running: BTreeMap::new(),
            }),
            ports_changed: watch::channel(0).0,
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
                    PortId::Replay(name) => self.replays.get(name).and_then(|r| r.recorded_with.clone()),
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

    /// Opens a port for the serial monitor. Events go to `sink` until the session is dropped
    /// or `sink` returns false.
    pub fn open_serial(
        self: &Arc<Self>,
        port: &str,
        baud: u32,
        sink: impl FnMut(PortEvent) -> bool + Send + 'static,
    ) -> Result<SerialSession, MachineError> {
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
        let stop = Arc::new(AtomicBool::new(false));
        let thread = spawn_reader(reader, self.clock.clone(), sink, Arc::clone(&stop));
        Ok(SerialSession {
            machine: Arc::downgrade(self),
            port: entry.id,
            writer: Mutex::new(writer),
            stop,
            thread: Some(thread),
        })
    }

    fn close_serial(&self, port: &PortId) {
        let mut st = self.state.lock().expect("machine lock");
        st.claims.release(port, &Claimant::Tool(SERIAL_MONITOR));
        self.ports_changed.send_modify(|n| *n += 1);
        self.reconcile(&mut st); // a part waiting for this port can start now
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
    port: PortId,
    writer: Mutex<Box<dyn PortWriter>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SerialSession {
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
    fn the_serial_monitor_and_parts_never_share_a_port() {
        let dir = tempfile::tempdir().unwrap();
        let m = machine(dir.path(), no_ports());
        let (tx, rx) = std::sync::mpsc::channel();
        let session = m.open_serial("replay:demo-ld19", 230_400, move |e| tx.send(e).is_ok()).unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), PortEvent::Data { .. }));
        let e = m.add_part(demo("lidar")).unwrap_err();
        assert_eq!(e.code, "port-claimed");
        assert!(e.message.contains("serial monitor"), "{e:?}");
        drop(session);
        m.add_part(demo("lidar")).unwrap();
        let e = m.open_serial("replay:demo-ld19", 230_400, |_| true).err().unwrap();
        assert_eq!(e.code, "port-claimed");
        m.shutdown();
    }
}
