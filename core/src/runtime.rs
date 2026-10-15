//! Layer 2: the runtime. Runs each part on its own supervised thread (docs/architecture.md §8).
//!
//! One part = one connector + one port. The supervisor opens the port, feeds the connector
//! bytes and ticks, publishes its readings and keeps its status. Two kinds of trouble, handled
//! differently on purpose:
//! - **The port fails** (missing, unplugged, closed): retried forever, at most every few seconds,
//!   because the user fixing a cable should bring the part back by itself.
//! - **The connector crashes** (a panic, which is a bug): restarted with growing backoff, and
//!   stopped after a few crashes in a row so a broken connector doesn't flap forever.

use crate::clock::SharedClock;
use crate::names::{PartName, SignalAddress};
use crate::ports::{PortEntry, PortEvent, PortOpener, spawn_reader};
use crate::router::{PublishError, Router};
use faber_capabilities::Reading;
use faber_connector::{Connector, Context, Definition, Health, Manifest, Problem, Settings};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// A part's place in its lifecycle, as the user sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartStatus {
    /// Its config can't work (unknown connector, bad settings). Needs a config change.
    Invalid(Problem),
    /// Its port is missing or used by something else.
    Conflict(Problem),
    Starting,
    Running,
    Degraded(Problem),
    /// Not working right now; Faber keeps trying.
    Failed(Problem),
    /// Faber gave up (the connector kept crashing).
    Stopped(Problem),
}

/// Every part's status, plus a change counter the API watches.
pub struct StatusBoard {
    statuses: Mutex<BTreeMap<PartName, PartStatus>>,
    changed: watch::Sender<u64>,
}

impl Default for StatusBoard {
    fn default() -> Self {
        StatusBoard { statuses: Mutex::new(BTreeMap::new()), changed: watch::channel(0).0 }
    }
}

impl StatusBoard {
    pub fn set(&self, part: &PartName, status: PartStatus) {
        let mut map = self.statuses.lock().expect("status lock");
        if map.get(part) != Some(&status) {
            map.insert(part.clone(), status);
            drop(map);
            self.changed.send_modify(|n| *n += 1);
        }
    }

    pub fn remove(&self, part: &PartName) {
        if self.statuses.lock().expect("status lock").remove(part).is_some() {
            self.changed.send_modify(|n| *n += 1);
        }
    }

    pub fn get(&self, part: &PartName) -> Option<PartStatus> {
        self.statuses.lock().expect("status lock").get(part).cloned()
    }

    /// Fires whenever any status changes.
    pub fn watch(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

/// Timing of the supervisor. Tests shrink these.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    /// How often the connector's `on_tick` runs.
    pub tick: Duration,
    /// First wait after a failure; doubles each time.
    pub backoff_start: Duration,
    /// Longest wait between retries of a failing port.
    pub port_retry_max: Duration,
    /// Crashes in a row before the part is stopped.
    pub max_crashes: u32,
    /// A run longer than this resets the crash count.
    pub healthy_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            tick: Duration::from_millis(100),
            backoff_start: Duration::from_secs(1),
            port_retry_max: Duration::from_secs(5),
            max_crashes: 5,
            healthy_after: Duration::from_secs(30),
        }
    }
}

/// What a part runs.
pub struct RunSpec {
    pub name: PartName,
    pub manifest: Manifest,
    pub create: fn(&Settings) -> Result<Box<dyn Connector>, faber_connector::SettingsError>,
    pub settings: Settings,
    pub port: PortEntry,
}

impl RunSpec {
    pub fn new(name: PartName, definition: &Definition, settings: Settings, port: PortEntry) -> RunSpec {
        RunSpec { name, manifest: definition.manifest.clone(), create: definition.create, settings, port }
    }
}

/// What every part shares.
#[derive(Clone)]
pub struct RunDeps {
    pub clock: SharedClock,
    pub opener: Arc<dyn PortOpener>,
    pub router: Arc<Router>,
    pub status: Arc<StatusBoard>,
    pub timing: Timing,
}

/// A running part. Stopping it (or dropping it) ends its thread and removes its signals.
pub struct PartHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PartHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PartHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts a part's supervisor thread.
pub fn start(spec: RunSpec, deps: RunDeps) -> PartHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name(format!("part:{}", spec.name))
        .spawn(move || supervise(spec, deps, thread_stop))
        .expect("spawn part thread");
    PartHandle { stop, thread: Some(thread) }
}

/// The connector's view of the core while it runs.
struct PartContext<'a> {
    part: &'a PartName,
    manifest: &'a Manifest,
    router: &'a Router,
    status: &'a StatusBoard,
    /// A connector bug found while it ran (publishing the wrong capability); ends the run.
    bug: Option<Problem>,
}

impl Context for PartContext<'_> {
    fn publish(&mut self, name: &str, reading: Reading) {
        if self.manifest.provide(name).is_none() {
            self.bug = Some(connector_bug(&format!(
                "it published to \"{name}\", which its manifest doesn't provide"
            )));
            return;
        }
        match self.router.publish(&SignalAddress::new(self.part.clone(), name), reading) {
            Ok(()) => {}
            Err(PublishError::WrongCapability { declared, got }) => {
                self.bug =
                    Some(connector_bug(&format!("it published {} on a {} signal", got.id(), declared.id())));
            }
            // Declared by `supervise` before the connector runs, so this can't happen.
            Err(PublishError::Undeclared) => unreachable!("signals are declared before the connector starts"),
        }
    }

    fn set_health(&mut self, health: Health) {
        let status = match health {
            Health::Starting => PartStatus::Starting,
            Health::Running => PartStatus::Running,
            Health::Degraded(p) => PartStatus::Degraded(p),
            Health::Failed(p) => PartStatus::Failed(p),
        };
        self.status.set(self.part, status);
    }
}

/// A part that needs a FaberLink bridge, which arrives in Phase 4 (docs/simulation.md, section 3).
pub fn needs_bridge(manifest: &Manifest) -> Problem {
    Problem::new(
        format!(
            "{} needs a bridge (a microcontroller board), which Faber can't use yet (Phase 4).",
            manifest.name
        ),
        "Tick Simulated for now: set simulate = \"manual\" or \"world\" instead of port in parts.toml.",
    )
}

fn connector_bug(what: &str) -> Problem {
    Problem::new(
        format!("The connector stopped because of a bug: {what}."),
        "Faber restarts it automatically. If this keeps happening, report it to the connector's author.",
    )
}

/// Sleeps for `d`, waking early if `stop` is set. Returns false if stopped.
fn wait(d: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep((until - Instant::now()).min(Duration::from_millis(20)));
    }
    !stop.load(Ordering::Relaxed)
}

enum RunEnd {
    Stopped,
    PortFailed(Problem),
    Crashed(Problem),
}

fn supervise(spec: RunSpec, deps: RunDeps, stop: Arc<AtomicBool>) {
    let addresses: Vec<SignalAddress> = spec
        .manifest
        .provides
        .iter()
        .map(|p| SignalAddress::new(spec.name.clone(), p.name.clone()))
        .collect();
    for (addr, provide) in addresses.iter().zip(&spec.manifest.provides) {
        deps.router.declare(addr.clone(), provide.capability);
    }
    if spec.manifest.port == faber_connector::PortRequirement::Bridge {
        deps.status.set(&spec.name, PartStatus::Invalid(needs_bridge(&spec.manifest)));
        for addr in &addresses {
            deps.router.remove(addr);
        }
        return;
    }
    let mut crashes = 0u32;
    let mut port_backoff = deps.timing.backoff_start;
    while !stop.load(Ordering::Relaxed) {
        deps.status.set(&spec.name, PartStatus::Starting);
        let connector = match (spec.create)(&spec.settings) {
            Ok(c) => c,
            Err(e) => {
                deps.status.set(
                    &spec.name,
                    PartStatus::Invalid(Problem::new(
                        format!("Setting \"{}\": {}", e.key, e.message),
                        e.hint,
                    )),
                );
                break; // needs a config change; nothing to retry
            }
        };
        let started = Instant::now();
        let end = run_once(&spec, &deps, connector, &stop);
        if started.elapsed() >= deps.timing.healthy_after {
            crashes = 0;
        }
        let pause = match end {
            RunEnd::Stopped => break,
            RunEnd::PortFailed(p) => {
                tracing::warn!(target: "core::runtime", part = %spec.name, "{} {}", p.message, p.hint);
                deps.status.set(&spec.name, PartStatus::Failed(p));
                let pause = port_backoff;
                port_backoff = (port_backoff * 2).min(deps.timing.port_retry_max);
                pause
            }
            RunEnd::Crashed(p) => {
                crashes += 1;
                tracing::error!(target: "core::runtime", part = %spec.name, "{} (crash {crashes})", p.message);
                if crashes >= deps.timing.max_crashes {
                    deps.status.set(
                        &spec.name,
                        PartStatus::Stopped(Problem::new(
                            format!("The connector crashed {crashes} times in a row, so Faber stopped it."),
                            "Check the log for the cause. Remove and add the part again to retry.",
                        )),
                    );
                    break;
                }
                deps.status.set(&spec.name, PartStatus::Failed(p));
                deps.timing.backoff_start * 2u32.pow(crashes - 1)
            }
        };
        if !wait(pause, &stop) {
            break;
        }
    }
    for addr in &addresses {
        deps.router.remove(addr);
    }
}

fn run_once(
    spec: &RunSpec,
    deps: &RunDeps,
    mut connector: Box<dyn Connector>,
    stop: &Arc<AtomicBool>,
) -> RunEnd {
    let baud = match spec.manifest.port {
        faber_connector::PortRequirement::Serial { baud } => baud,
        faber_connector::PortRequirement::Bridge => unreachable!("`supervise` never runs a bridge part"),
    };
    let (reader, _writer) = match deps.opener.open(&spec.port, baud) {
        Ok(rw) => rw,
        Err(e) => {
            return RunEnd::PortFailed(Problem::new(
                format!("Could not open {}: {e}", spec.port.label),
                "Check that it is plugged in and that no other program (for example a serial terminal) uses it.",
            ));
        }
    };
    let (tx, rx) = mpsc::channel();
    let reader_stop = Arc::new(AtomicBool::new(false));
    let reader_thread =
        spawn_reader(reader, Arc::clone(&deps.clock), move |e| tx.send(e).is_ok(), Arc::clone(&reader_stop));
    let mut ctx = PartContext {
        part: &spec.name,
        manifest: &spec.manifest,
        router: &deps.router,
        status: &deps.status,
        bug: None,
    };
    let mut next_tick = Instant::now();
    let end = loop {
        if stop.load(Ordering::Relaxed) {
            break RunEnd::Stopped;
        }
        let event = rx.recv_timeout(next_tick.saturating_duration_since(Instant::now()));
        let call = match event {
            Ok(PortEvent::Data { t_us, bytes }) => {
                catch_unwind(AssertUnwindSafe(|| connector.on_bytes(&mut ctx, t_us, &bytes)))
            }
            Ok(PortEvent::Closed { reason }) => {
                break RunEnd::PortFailed(Problem::new(
                    format!("{} stopped working: {reason}", spec.port.label),
                    "Check the cable and the power. Faber reconnects as soon as the port is back.",
                ));
            }
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => {
                break RunEnd::PortFailed(Problem::new(
                    format!("{} stopped working.", spec.port.label),
                    "Check the cable and the power. Faber reconnects as soon as the port is back.",
                ));
            }
        };
        if Instant::now() >= next_tick && call.is_ok() {
            let now = deps.clock.now_us();
            if let Err(panic) = catch_unwind(AssertUnwindSafe(|| connector.on_tick(&mut ctx, now))) {
                break RunEnd::Crashed(connector_bug(&panic_message(&panic)));
            }
            next_tick = Instant::now() + deps.timing.tick;
        }
        if let Err(panic) = call {
            break RunEnd::Crashed(connector_bug(&panic_message(&panic)));
        }
        if let Some(bug) = ctx.bug.take() {
            break RunEnd::Crashed(bug);
        }
    };
    reader_stop.store(true, Ordering::Relaxed);
    drop(rx);
    let _ = reader_thread.join();
    end
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    let text = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into());
    format!("it panicked ({text})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MonotonicClock;
    use crate::ports::{PortId, PortKind, PortReader, PortWriter};
    use faber_capabilities::{Capability, laser_scan};
    use faber_connector::{PortRequirement, Provide, SettingsError};
    use std::io;
    use std::sync::atomic::AtomicU32;

    /// A port that hands out the given chunks, then either ends or stays silent.
    struct FakeReader {
        chunks: Vec<Vec<u8>>,
        end_with_error: bool,
    }

    impl PortReader for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.chunks.is_empty() {
                if self.end_with_error {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "unplugged"));
                }
                std::thread::sleep(Duration::from_millis(5));
                return Ok(0);
            }
            let c = self.chunks.remove(0);
            buf[..c.len()].copy_from_slice(&c);
            Ok(c.len())
        }
    }

    struct Sink;
    impl PortWriter for Sink {
        fn write(&mut self, _: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    struct FakeOpener {
        chunks: Vec<Vec<u8>>,
        end_with_error: bool,
        fail_to_open: bool,
        opens: AtomicU32,
    }

    impl FakeOpener {
        fn new(chunks: Vec<Vec<u8>>) -> Self {
            FakeOpener { chunks, end_with_error: false, fail_to_open: false, opens: AtomicU32::new(0) }
        }
    }

    impl PortOpener for FakeOpener {
        fn open(&self, _: &PortEntry, _: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            if self.fail_to_open {
                return Err(io::Error::new(io::ErrorKind::NotFound, "no such port"));
            }
            let reader = FakeReader { chunks: self.chunks.clone(), end_with_error: self.end_with_error };
            Ok((Box::new(reader), Box::new(Sink)))
        }
    }

    /// Publishes one scan per byte; byte 0xFF panics; byte 0xEE publishes to an undeclared name.
    struct Echo;
    impl Connector for Echo {
        fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: u64, bytes: &[u8]) {
            for &b in bytes {
                match b {
                    0xFF => panic!("bad byte"),
                    0xEE => ctx.publish(
                        "other",
                        Reading::LaserScan(laser_scan::Reading::even_bins(t_us, 0.0, 0.1, 0.1, vec![])),
                    ),
                    _ => {
                        ctx.set_health(Health::Running);
                        let r = laser_scan::Reading::even_bins(u64::from(b), 0.0, 0.1, 0.1, vec![1.0]);
                        ctx.publish("laser-scan", Reading::LaserScan(r));
                    }
                }
            }
        }
    }

    fn create_echo(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
        match s.keys().next() {
            Some(k) => {
                Err(SettingsError { key: k.clone(), message: "unknown".into(), hint: "remove it".into() })
            }
            None => Ok(Box::new(Echo)),
        }
    }

    fn spec(settings: Settings) -> RunSpec {
        RunSpec {
            name: PartName::parse("echo").unwrap(),
            manifest: Manifest {
                id: "echo".into(),
                name: "Echo".into(),
                version: "0".into(),
                port: PortRequirement::Serial { baud: 9600 },
                provides: vec![Provide::default_for(Capability::LaserScan)],
                usb_hints: vec![],
            },
            create: create_echo,
            settings,
            port: PortEntry {
                id: PortId::Serial("FAKE".into()),
                kind: PortKind::Serial,
                label: "FAKE".into(),
                path: Some("FAKE".into()),
                usb: None,
            },
        }
    }

    fn deps(opener: Arc<FakeOpener>) -> RunDeps {
        RunDeps {
            clock: Arc::new(MonotonicClock::new()),
            opener,
            router: Arc::new(Router::new()),
            status: Arc::new(StatusBoard::default()),
            timing: Timing {
                tick: Duration::from_millis(5),
                backoff_start: Duration::from_millis(5),
                port_retry_max: Duration::from_millis(20),
                max_crashes: 3,
                healthy_after: Duration::from_secs(30),
            },
        }
    }

    fn name() -> PartName {
        PartName::parse("echo").unwrap()
    }

    /// Waits until the part's status matches, or fails after 2 s.
    fn wait_for(status: &StatusBoard, f: impl Fn(&PartStatus) -> bool) -> PartStatus {
        let start = Instant::now();
        loop {
            if let Some(s) = status.get(&name())
                && f(&s)
            {
                return s;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "timed out; last status {:?}",
                status.get(&name())
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[tokio::test]
    async fn runs_a_part_and_publishes_its_readings() {
        let d = deps(Arc::new(FakeOpener::new(vec![vec![1, 2, 3]])));
        let addr = SignalAddress::new(name(), "laser-scan");
        let handle = start(spec(Settings::new()), d.clone());
        wait_for(&d.status, |s| *s == PartStatus::Running);
        let mut sub = d.router.subscribe(&addr).expect("declared");
        let r = tokio::time::timeout(Duration::from_secs(1), sub.next()).await.unwrap().unwrap();
        assert_eq!(r.t_us(), 3, "latest reading");
        handle.stop();
        assert!(d.router.subscribe(&addr).is_none(), "signals are removed when the part stops");
    }

    #[test]
    fn bad_settings_make_the_part_invalid_without_retrying() {
        let opener = Arc::new(FakeOpener::new(vec![]));
        let d = deps(Arc::clone(&opener));
        let mut s = Settings::new();
        s.insert("x".into(), 1.into());
        let _h = start(spec(s), d.clone());
        let PartStatus::Invalid(p) = wait_for(&d.status, |s| matches!(s, PartStatus::Invalid(_))) else {
            unreachable!()
        };
        assert!(p.message.contains("\"x\""));
        assert_eq!(p.hint, "remove it");
        assert_eq!(opener.opens.load(Ordering::SeqCst), 0, "the port is never opened");
    }

    #[test]
    fn a_part_that_needs_a_bridge_is_invalid_and_its_port_never_opened() {
        let opener = Arc::new(FakeOpener::new(vec![]));
        let d = deps(Arc::clone(&opener));
        let mut sp = spec(Settings::new());
        sp.manifest.port = PortRequirement::Bridge;
        let _h = start(sp, d.clone());
        let PartStatus::Invalid(p) = wait_for(&d.status, |s| matches!(s, PartStatus::Invalid(_))) else {
            unreachable!()
        };
        assert!(p.message.contains("needs a bridge"), "{p:?}");
        assert!(p.hint.contains("Simulated"), "{p:?}");
        assert_eq!(opener.opens.load(Ordering::SeqCst), 0);
        assert!(d.router.subscribe(&SignalAddress::new(name(), "laser-scan")).is_none());
    }

    #[test]
    fn a_missing_port_is_retried() {
        let mut o = FakeOpener::new(vec![]);
        o.fail_to_open = true;
        let opener = Arc::new(o);
        let d = deps(Arc::clone(&opener));
        let _h = start(spec(Settings::new()), d.clone());
        let PartStatus::Failed(p) = wait_for(&d.status, |s| matches!(s, PartStatus::Failed(_))) else {
            unreachable!()
        };
        assert!(p.message.contains("Could not open FAKE"));
        let start_wait = Instant::now();
        while opener.opens.load(Ordering::SeqCst) < 4 {
            assert!(start_wait.elapsed() < Duration::from_secs(2), "keeps retrying");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn an_unplugged_port_fails_and_reconnects() {
        let mut o = FakeOpener::new(vec![vec![1]]);
        o.end_with_error = true;
        let opener = Arc::new(o);
        let d = deps(Arc::clone(&opener));
        let _h = start(spec(Settings::new()), d.clone());
        let PartStatus::Failed(p) = wait_for(&d.status, |s| matches!(s, PartStatus::Failed(_))) else {
            unreachable!()
        };
        assert!(p.message.contains("stopped working"), "{p:?}");
        assert!(p.hint.contains("reconnects"));
        let start_wait = Instant::now();
        while opener.opens.load(Ordering::SeqCst) < 2 {
            assert!(start_wait.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_crashing_connector_is_restarted_then_stopped() {
        let opener = Arc::new(FakeOpener::new(vec![vec![0xFF]]));
        let d = deps(Arc::clone(&opener));
        let _h = start(spec(Settings::new()), d.clone());
        let PartStatus::Stopped(p) = wait_for(&d.status, |s| matches!(s, PartStatus::Stopped(_))) else {
            unreachable!()
        };
        assert!(p.message.contains("crashed 3 times"));
        assert_eq!(opener.opens.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn publishing_to_an_undeclared_name_is_treated_as_a_connector_bug() {
        let opener = Arc::new(FakeOpener::new(vec![vec![0xEE]]));
        let d = deps(opener);
        let _h = start(spec(Settings::new()), d.clone());
        let s = wait_for(&d.status, |s| matches!(s, PartStatus::Stopped(_) | PartStatus::Failed(_)));
        let (PartStatus::Failed(p) | PartStatus::Stopped(p)) = s else { unreachable!() };
        assert!(p.message.contains("bug") || p.message.contains("crashed"), "{p:?}");
    }

    #[test]
    fn status_board_only_notifies_on_change() {
        let b = StatusBoard::default();
        let rx = b.watch();
        b.set(&name(), PartStatus::Running);
        b.set(&name(), PartStatus::Running);
        assert_eq!(*rx.borrow(), 1);
        b.remove(&name());
        assert_eq!(*rx.borrow(), 2);
        assert_eq!(b.get(&name()), None);
    }
}
