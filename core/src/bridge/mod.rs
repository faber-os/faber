//! Layer 3: the `faber-bridge` connector at work. Runs one FaberLink board (a bridge) on its
//! serial port (docs/faberlink.md §9):
//!
//! - It owns the port and a [`faberlink::Session`]: hello, describe, a heartbeat every 50 ms that
//!   carries Faber's e-stop, and the board's status.
//! - The board's parts become **sub-parts**, `<bridge>.<name>` (`mega.left`), with their own
//!   signals and health. Readings go to their signals. Commands reach the board only through
//!   [`BridgeHandle::command`], which the machine calls after it has checked the lease; whenever
//!   a lease ends the machine calls [`BridgeHandle::safe`].
//! - Health says what is wrong and what to try: no answer, a version mismatch, a board that went
//!   quiet, a board that stopped its motors by itself.
//! - The port is reopened with backoff, and the board is described again every time it connects,
//!   so new firmware with other parts is picked up.
//!
//! The thread never takes the machine's lock: it reports through [`BridgeOutlet`], whose calls
//! return at once, so the machine may call the handle while it holds its own lock.

mod board;
mod probe;
#[cfg(test)]
pub(crate) mod testboard;
#[cfg(test)]
mod tests;

pub use board::{LinkState, SubPart};
pub use probe::{ProbeOutcome, ProbeTiming, probe};

use crate::clock::SharedClock;
use crate::connectors::bridge::{BAUD, ESTOP_SIGNAL};
use crate::names::{PartName, SignalAddress};
use crate::ports::{PortEntry, PortEvent, PortOpener, PortReader, PortWriter, spawn_reader};
use crate::router::Router;
use faber_capabilities::{Capability, Command, Reading, estop};
use faber_connector::{Applied, CommandError, Health, Problem};
use faberlink::message::{Cause, LogLevel};
use faberlink::session::{BoardInfo, CommandError as LinkError, Event, Session, SessionConfig, State};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Where a bridge reports. Every call returns at once (no machine lock behind it).
pub trait BridgeOutlet: Send + Sync {
    /// The health of the bridge itself, or of one of its sub-parts.
    fn set_health(&self, part: &PartName, health: Health);
    /// A sub-part left the board (new firmware), or the bridge stopped: it has no health any more.
    fn forget(&self, part: &PartName);
    /// Something only the machine can act on.
    fn event(&self, event: BridgeEvent);
}

/// What a bridge needs the machine to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeEvent {
    /// The board was described with other parts than before (new firmware): leases on parts that
    /// are gone or changed must end.
    PartsChanged(PartName),
    /// The board's own stop button (a hardware e-stop) was pressed: engage Faber's e-stop.
    HardwareEstop { bridge: PartName, reason: String },
}

/// Timing of a bridge. Tests shrink the waits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeTiming {
    /// First wait before reopening a port that failed; doubles each time.
    pub backoff_start: Duration,
    /// Longest wait between tries, and between hellos to a board with another protocol version.
    pub retry_max: Duration,
    /// No answer to HELLO for this long, and the bridge says the board doesn't answer. Opening
    /// the port resets most Arduinos, which then take about a second to start.
    pub handshake: Duration,
    /// No STATUS from a connected board for this long, and the bridge says so (a board sends
    /// one every 100 ms).
    pub status_silence: Duration,
    pub session: SessionConfig,
}

impl Default for BridgeTiming {
    fn default() -> Self {
        BridgeTiming {
            backoff_start: Duration::from_secs(1),
            retry_max: Duration::from_secs(5),
            handshake: Duration::from_millis(2500),
            status_silence: Duration::from_millis(500),
            session: SessionConfig::default(),
        }
    }
}

pub struct BridgeSpec {
    pub name: PartName,
    pub port: PortEntry,
}

#[derive(Clone)]
pub struct BridgeDeps {
    pub clock: SharedClock,
    pub opener: Arc<dyn PortOpener>,
    pub router: Arc<Router>,
    pub outlet: Arc<dyn BridgeOutlet>,
    pub timing: BridgeTiming,
}

/// The board and the link, as Parts shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardView {
    pub link: LinkState,
    /// What the board said about itself, once it has answered.
    pub board: Option<BoardInfo>,
    /// The board's own stop button is latched: reset it on the board, then release in Faber.
    pub hardware_estop: bool,
    /// The board stopped its actuators because Faber's heartbeat didn't reach it in time.
    pub failsafe: bool,
}

/// Faber's e-stop as a bridge passes it on: who engaged it and why, or `None` when released.
pub type EstopLevel = Option<(estop::Source, String)>;

/// One board's link, shared by its thread and its handle.
struct Link {
    bridge: PartName,
    session: Option<Session>,
    writer: Option<Box<dyn PortWriter>>,
    subs: Vec<SubPart>,
    state: LinkState,
    board: Option<BoardInfo>,
    /// The board's stop button is latched, as the board last said (STATUS, or an ESTOP from it).
    hardware_estop: bool,
    failsafe: bool,
    /// Faber's e-stop, given to every new session so its first heartbeats already carry it.
    estop: EstopLevel,
}

impl Link {
    /// Writes what the session has to send. A failed write means the port is gone; the reader
    /// notices too, and the thread opens it again.
    fn flush(&mut self) -> std::io::Result<()> {
        let (Some(session), Some(writer)) = (self.session.as_mut(), self.writer.as_mut()) else {
            return Ok(());
        };
        let out = session.take_output();
        if out.is_empty() { Ok(()) } else { writer.write(&out) }
    }

    fn sub(&self, name: &PartName) -> Option<&SubPart> {
        self.subs.iter().find(|s| &s.name == name)
    }

    fn not_connected(&self, part: &PartName) -> CommandError {
        CommandError::NotReady(Problem::new(
            format!("\"{}\" isn't connected to its board right now, so \"{part}\" can't move.", self.bridge),
            "Check the board's cable and power; Parts shows what is wrong. It reconnects by itself.",
        ))
    }

    /// The board is out of reach: say why, and stop every sub-part. Its stop button can't hold
    /// anything while it is away, so it no longer blocks releasing the e-stop; a board that comes
    /// back with the button still latched engages the e-stop again.
    fn lose(&mut self, outlet: &dyn BridgeOutlet, state: LinkState, problem: Problem) {
        self.state = state;
        self.hardware_estop = false;
        self.failsafe = false;
        let stopped = match state {
            LinkState::Incompatible => Problem::new(
                format!("Stopped: \"{}\" can't talk to its board.", self.bridge),
                "The board's own line in Parts says why.",
            ),
            _ => Problem::new(
                format!("Stopped: \"{}\" can't reach its board right now.", self.bridge),
                "It comes back by itself as soon as the board answers again.",
            ),
        };
        for s in &self.subs {
            outlet.set_health(&s.name, Health::Failed(stopped.clone()));
        }
        tracing::warn!(target: "core::bridge", part = %self.bridge, "{} {}", problem.message, problem.hint);
        outlet.set_health(&self.bridge, Health::Failed(problem));
    }
}

/// A running bridge. Stopping (or dropping) it closes the port, removes its signals and forgets
/// its sub-parts.
pub struct BridgeHandle {
    link: Arc<Mutex<Link>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl BridgeHandle {
    fn link(&self) -> MutexGuard<'_, Link> {
        self.link.lock().expect("bridge lock")
    }

    /// The board's parts as sub-parts, from its last describe. They stay while the board is
    /// away (failed, saying why), so Parts keeps showing them.
    pub fn sub_parts(&self) -> Vec<SubPart> {
        self.link().subs.clone()
    }

    pub fn sub_part(&self, name: &PartName) -> Option<SubPart> {
        self.link().sub(name).cloned()
    }

    pub fn view(&self) -> BoardView {
        let l = self.link();
        BoardView {
            link: l.state,
            board: l.board.clone(),
            hardware_estop: l.hardware_estop,
            failsafe: l.failsafe,
        }
    }

    /// True while the board's own stop button is latched.
    pub fn hardware_estop_latched(&self) -> bool {
        self.link().hardware_estop
    }

    /// Sends a command to a sub-part. The machine calls this only after checking the lease and
    /// clamping the command to the part's info; nothing else sends COMMAND to the board.
    pub fn command(&self, part: &PartName, command: Command) -> Result<Applied, CommandError> {
        let mut l = self.link();
        let Some(id) = l.sub(part).map(|s| s.id) else {
            return Err(CommandError::NotReady(Problem::new(
                format!("The board has no part \"{}\" right now.", part.local()),
                "Check the sketch on the board; Parts lists the parts it has.",
            )));
        };
        let Some(session) = l.session.as_mut() else { return Err(l.not_connected(part)) };
        match session.command(id, command, true) {
            Ok(()) => {}
            Err(LinkError::WrongCapability { .. }) => return Err(CommandError::Unsupported),
            Err(LinkError::TooLargeForBoard { size, max }) => {
                return Err(CommandError::NotReady(Problem::new(
                    format!("The command is {size} bytes, but the board takes at most {max}."),
                    "Build the sketch for a board with more memory, or raise FABERLINK_MAX_PAYLOAD.",
                )));
            }
            Err(_) => return Err(l.not_connected(part)),
        }
        match l.flush() {
            Ok(()) => Ok(Applied { command }),
            Err(_) => Err(l.not_connected(part)),
        }
    }

    /// Puts a sub-part in its safe state now (SAFE). A board that isn't connected has stopped
    /// everything by itself (no heartbeat, no motion), so there is nothing more to do then.
    pub fn safe(&self, part: &PartName) {
        let mut l = self.link();
        let Some(id) = l.sub(part).map(|s| s.id) else { return };
        if let Some(session) = l.session.as_mut()
            && session.safe(Some(id)).is_ok()
        {
            let _ = l.flush();
        }
    }

    /// Faber's e-stop: an ESTOP frame (and SAFE for every part) now, and the level in every
    /// heartbeat from now on.
    pub fn set_estop(&self, level: EstopLevel) {
        let mut l = self.link();
        l.estop = level.clone();
        let Some(session) = l.session.as_mut() else { return };
        match level {
            Some((source, reason)) => {
                session.set_estop(true, source, &reason);
                let _ = session.safe(None);
            }
            None => session.set_estop(false, estop::Source::Shell, "Released"),
        }
        let _ = l.flush();
    }

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

impl Drop for BridgeHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts a bridge's thread: it opens the port and keeps the board connected until stopped.
/// `estop` is Faber's e-stop right now.
pub fn start(spec: BridgeSpec, deps: BridgeDeps, estop: EstopLevel) -> BridgeHandle {
    let link = Arc::new(Mutex::new(Link {
        bridge: spec.name.clone(),
        session: None,
        writer: None,
        subs: Vec::new(),
        state: LinkState::Connecting,
        board: None,
        hardware_estop: false,
        failsafe: false,
        estop,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (thread_link, thread_stop) = (Arc::clone(&link), Arc::clone(&stop));
    let thread = std::thread::Builder::new()
        .name(format!("bridge:{}", spec.name))
        .spawn(move || supervise(spec, deps, thread_link, thread_stop))
        .expect("spawn bridge thread");
    BridgeHandle { link, stop, thread: Some(thread) }
}

/// The address of a sub-part's signal (`mega.left/motor`), or the bridge's own (`mega/estop`).
fn address(part: &PartName, signal: &str) -> SignalAddress {
    SignalAddress::new(part.clone(), signal)
}

/// Waits `d`, waking early when stopped. Returns false if stopped.
fn wait(d: Duration, stop: &AtomicBool) -> bool {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep((until - Instant::now()).min(Duration::from_millis(10)));
    }
    !stop.load(Ordering::Relaxed)
}

fn supervise(spec: BridgeSpec, deps: BridgeDeps, link: Arc<Mutex<Link>>, stop: Arc<AtomicBool>) {
    let own = address(&spec.name, ESTOP_SIGNAL);
    deps.router.declare(own.clone(), Capability::Estop);
    deps.outlet.set_health(&spec.name, Health::Starting);
    let mut backoff = deps.timing.backoff_start;
    while !stop.load(Ordering::Relaxed) {
        let answered = match deps.opener.open(&spec.port, BAUD) {
            Err(e) => {
                let problem = Problem::new(
                    format!("Could not open {}: {e}", spec.port.label),
                    "Check that the board is plugged in and that no other program (for example the Arduino IDE's serial monitor) uses the port.",
                );
                link.lock().expect("bridge lock").lose(&*deps.outlet, LinkState::Lost, problem);
                false
            }
            Ok((reader, writer)) => Run::new(&spec, &deps, &link, &stop).go(reader, writer),
        };
        if answered {
            backoff = deps.timing.backoff_start;
        }
        if !wait(backoff, &stop) {
            break;
        }
        backoff = (backoff * 2).min(deps.timing.retry_max);
    }
    let subs = std::mem::take(&mut link.lock().expect("bridge lock").subs);
    for s in &subs {
        if let Some(p) = &s.provide {
            deps.router.remove(&address(&s.name, &p.name));
        }
        deps.outlet.forget(&s.name);
    }
    deps.router.remove(&own);
}

/// One time the port is open.
struct Run<'a> {
    spec: &'a BridgeSpec,
    deps: &'a BridgeDeps,
    link: &'a Mutex<Link>,
    stop: &'a AtomicBool,
    /// When the session last started asking (opening the port, or a restart), core clock.
    since_us: u64,
    /// When the last STATUS arrived.
    last_status_us: u64,
    /// The board's count of dropped frames, and when it last grew.
    rx_bad: (u16, Option<u64>),
    /// The bridge's health as last reported, so it is only reported when it changes.
    reported: Option<Health>,
    /// The board answered at least once.
    answered: bool,
}

impl<'a> Run<'a> {
    fn new(spec: &'a BridgeSpec, deps: &'a BridgeDeps, link: &'a Mutex<Link>, stop: &'a AtomicBool) -> Self {
        Run {
            spec,
            deps,
            link,
            stop,
            since_us: 0,
            last_status_us: 0,
            rx_bad: (0, None),
            reported: None,
            answered: false,
        }
    }

    fn lock(&self) -> MutexGuard<'a, Link> {
        self.link.lock().expect("bridge lock")
    }

    /// Runs the link until the port closes or the bridge stops. Returns true if the board
    /// answered, so the next try starts with a short wait again.
    fn go(mut self, reader: Box<dyn PortReader>, writer: Box<dyn PortWriter>) -> bool {
        let (tx, rx) = mpsc::channel();
        let reader_stop = Arc::new(AtomicBool::new(false));
        let reader_thread = spawn_reader(
            reader,
            Arc::clone(&self.deps.clock),
            move |e| tx.send(e).is_ok(),
            Arc::clone(&reader_stop),
        );
        self.since_us = self.deps.clock.now_us();
        {
            let mut l = self.lock();
            let mut session = Session::new(self.deps.timing.session.clone(), self.since_us);
            if let Some((source, reason)) = &l.estop {
                session.set_estop(true, *source, reason);
            }
            l.session = Some(session);
            l.writer = Some(writer);
            l.state = LinkState::Connecting;
            let _ = l.flush();
        }
        let closed = loop {
            if self.stop.load(Ordering::Relaxed) {
                break None;
            }
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(PortEvent::Data { t_us, bytes }) => {
                    if let Some(s) = self.lock().session.as_mut() {
                        s.receive(&bytes, t_us);
                    }
                }
                Ok(PortEvent::Closed { reason }) => break Some(reason),
                Err(RecvTimeoutError::Timeout) if reader_thread.is_finished() => break Some(String::new()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break Some(String::new()),
            }
            if let Err(e) = self.step() {
                break Some(e.to_string());
            }
        };
        reader_stop.store(true, Ordering::Relaxed);
        let mut l = self.lock();
        l.session = None;
        l.writer = None;
        if let Some(reason) = closed {
            let label = &self.spec.port.label;
            let message = match reason.as_str() {
                "" => format!("{label} stopped working."),
                r => format!("{label} stopped working: {r}"),
            };
            let problem = Problem::new(
                message,
                "The board was probably unplugged or lost power. Faber reconnects as soon as it is back.",
            );
            l.lose(&*self.deps.outlet, LinkState::Lost, problem);
        }
        drop(l);
        let _ = reader_thread.join();
        self.answered
    }

    /// Runs the session's timers, writes what it sends and handles what the board said.
    fn step(&mut self) -> std::io::Result<()> {
        let now = self.deps.clock.now_us();
        let mut l = self.lock();
        let Some(session) = l.session.as_mut() else { return Ok(()) };
        if session.state() == State::Failed
            && now.saturating_sub(self.since_us) >= us(self.deps.timing.retry_max)
        {
            // The firmware may have been updated meanwhile: ask again.
            session.restart(now);
            self.since_us = now;
        }
        session.tick(now);
        let events = session.take_events();
        let state = session.state();
        l.flush()?;
        for e in events {
            self.on_event(&mut l, e, now);
        }
        self.watch(&mut l, state, now);
        Ok(())
    }

    fn on_event(&mut self, l: &mut Link, event: Event, now: u64) {
        let bridge = &self.spec.name;
        match event {
            Event::Connected(info) => {
                self.answered = true;
                tracing::info!(
                    target: "core::bridge",
                    part = %bridge,
                    "connected to {} ({} {}, FaberLink {})",
                    info.board,
                    info.firmware,
                    info.firmware_version,
                    info.version
                );
                l.board = Some(info);
                l.state = LinkState::Describing;
                self.last_status_us = now;
            }
            Event::Described(parts) => self.described(l, board::sub_parts(bridge, &parts)),
            Event::Reading { part, reading } => {
                if let Some(s) = l.subs.iter().find(|s| s.id == part)
                    && let Some(p) = &s.provide
                {
                    let _ = self.deps.router.publish(&address(&s.name, &p.name), reading);
                }
            }
            Event::Applied(a) => {
                if a.cause != Cause::Command
                    && let Some(s) = l.subs.iter().find(|s| s.id == a.part)
                {
                    let why = board::cause_words(a.cause);
                    tracing::info!(target: "core::bridge", part = %bridge, "{} stopped: {why}", s.name);
                }
            }
            Event::Status(s) => {
                self.last_status_us = now;
                if s.rx_bad > self.rx_bad.0 {
                    self.rx_bad = (s.rx_bad, Some(now));
                }
                l.failsafe = s.failsafe;
                if s.hardware_estop && !l.hardware_estop {
                    self.hardware_estop(l, "Stop button pressed on the board", now);
                } else if !s.hardware_estop && l.hardware_estop {
                    l.hardware_estop = false;
                    self.publish_estop(false, "Reset on the board", now);
                }
            }
            Event::Estop(e) if e.engaged && !l.hardware_estop => {
                let what =
                    if e.reason.is_empty() { "Stop button pressed on the board".into() } else { e.reason };
                self.hardware_estop(l, &what, now);
            }
            // The board only ever engages; a release comes from Faber.
            Event::Estop(_) => {}
            Event::Log(line) => {
                let text = line.text;
                match line.level {
                    LogLevel::Info => tracing::info!(target: "core::bridge", part = %bridge, "{text}"),
                    LogLevel::Warn => tracing::warn!(target: "core::bridge", part = %bridge, "{text}"),
                    LogLevel::Error => tracing::error!(target: "core::bridge", part = %bridge, "{text}"),
                }
            }
            Event::BoardLost => {
                let who = l
                    .board
                    .as_ref()
                    .map_or("The board".to_string(), |b| format!("The board \"{}\"", b.board));
                let problem = Problem::new(
                    format!("{who} stopped answering."),
                    "Check the cable and the board's power. If it keeps happening, the sketch may keep loop() busy too long: link.poll() must run often.",
                );
                self.reported = Some(Health::Failed(problem.clone()));
                l.lose(&*self.deps.outlet, LinkState::Lost, problem);
                self.since_us = now;
            }
            Event::Failed(e) => {
                let problem = Problem::new(
                    e.to_string(),
                    "Faber asks again every few seconds, so the board connects as soon as both speak the same version.",
                );
                self.reported = Some(Health::Failed(problem.clone()));
                l.lose(&*self.deps.outlet, LinkState::Incompatible, problem);
                self.since_us = now;
            }
        }
    }

    /// The board described its parts: they become sub-parts, with signals.
    fn described(&mut self, l: &mut Link, subs: Vec<SubPart>) {
        let changed = subs != l.subs;
        let same = |a: &SubPart, b: &SubPart| a.name == b.name && a.provide == b.provide;
        for old in &l.subs {
            if !subs.iter().any(|s| same(s, old))
                && let Some(p) = &old.provide
            {
                self.deps.router.remove(&address(&old.name, &p.name));
            }
            if !subs.iter().any(|s| s.name == old.name) {
                self.deps.outlet.forget(&old.name);
            }
        }
        for new in &subs {
            if !l.subs.iter().any(|s| same(s, new))
                && let Some(p) = &new.provide
            {
                self.deps.router.declare(address(&new.name, &p.name), p.capability);
            }
        }
        for s in &subs {
            self.deps.outlet.set_health(&s.name, board::sub_health(s));
        }
        l.subs = subs;
        l.state = LinkState::Ready;
        if changed {
            self.deps.outlet.event(BridgeEvent::PartsChanged(self.spec.name.clone()));
        }
    }

    /// The board's stop button was pressed: Faber's e-stop engages, and can't be released until
    /// the button is reset on the board (docs/faberlink.md §5, rule 3).
    fn hardware_estop(&self, l: &mut Link, what: &str, now: u64) {
        l.hardware_estop = true;
        let reason = format!("{what} (\"{}\")", self.spec.name);
        self.publish_estop(true, &reason, now);
        self.deps.outlet.event(BridgeEvent::HardwareEstop { bridge: self.spec.name.clone(), reason });
    }

    fn publish_estop(&self, engaged: bool, reason: &str, now: u64) {
        let r = estop::Reading { t_us: now, engaged, source: estop::Source::Part, reason: reason.into() };
        let _ = self.deps.router.publish(&address(&self.spec.name, ESTOP_SIGNAL), Reading::Estop(r));
    }

    /// Keeps the bridge's health current: still asking, no answer, status gone quiet, or what
    /// the board says about itself.
    fn watch(&mut self, l: &mut Link, state: State, now: u64) {
        match (state, l.state) {
            (State::Connecting, LinkState::Connecting)
                if now.saturating_sub(self.since_us) >= us(self.deps.timing.handshake) =>
            {
                l.state = LinkState::NoAnswer;
                self.report(Health::Failed(Problem::new(
                    format!("No FaberLink answer from {}.", self.spec.port.label),
                    "Is the board running a FaberLink sketch, and is the baud right (115200, as in link.begin(115200, …))? Faber keeps asking.",
                )));
            }
            (State::Connecting, LinkState::Connecting) | (State::Describing, _) => {
                self.report(Health::Starting)
            }
            (State::Ready, _) => {
                let silent = now.saturating_sub(self.last_status_us) >= us(self.deps.timing.status_silence);
                let recent_bad =
                    self.rx_bad.1.filter(|t| now.saturating_sub(*t) < 5_000_000).map(|_| self.rx_bad.0);
                self.report(board::bridge_health(l, silent, recent_bad));
            }
            // Lost, no answer or another version: the failure stays until the board answers.
            _ => {}
        }
    }

    fn report(&mut self, health: Health) {
        if self.reported.as_ref() != Some(&health) {
            self.deps.outlet.set_health(&self.spec.name, health.clone());
            self.reported = Some(health);
        }
    }
}

fn us(d: Duration) -> u64 {
    d.as_micros() as u64
}
