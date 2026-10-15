//! Layer 3: runs wasm connectors as sandboxed WebAssembly components (docs/packages.md §3–4,
//! D07, D09).
//!
//! A wasm connector is a [`Definition`] like any other: the runtime supervises it, restarts it
//! with backoff and shows its health exactly as for a built-in one. Inside, each part gets its own
//! component instance in its own [`Store`], called on the part's own thread. The sandbox:
//! - **What it can call:** only `faber:connector/context` (publish, set-health, log). A component
//!   that imports anything else (WASI files, network, clocks, …) is refused when it is loaded.
//! - **What it can publish:** only the signals its manifest declares. Anything else is refused,
//!   logged, and stops the part like a bug in a built-in connector.
//! - **Memory:** at most [`Limits::memory_bytes`] per instance.
//! - **Time:** at most [`Limits::call_time`] per call, by epoch interruption.
//!
//! A trap or a limit ends the call by unwinding with the [`Problem`] to show, which the runtime
//! treats as a crash (`runtime::crash_problem`): the part fails with that message and restarts.

mod convert;
mod installed;

pub use installed::load_installed;

use faber_capabilities::{Command, Reading, Timestamp};
use faber_connector::{
    Applied, CommandError, Connector, ConnectorKind, Context, Definition, Health, Manifest, Problem,
    Settings, SettingsError,
};
use std::panic::resume_unwind;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store, Trap};

#[allow(clippy::all)]
mod bindings {
    wasmtime::component::bindgen!({
        path: ["../wit/capabilities", "../wit/connector"],
        world: "faber:connector/connector@0.1.0",
    });
}

use bindings::faber::connector::types as wit;

/// What one instance may use (docs/packages.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Linear memory, per instance.
    pub memory_bytes: usize,
    /// Wall time per call into the component.
    pub call_time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { memory_bytes: 32 << 20, call_time: Duration::from_millis(20) }
    }
}

/// How often the engine's epoch advances; a call's deadline is counted in these.
const EPOCH_TICK: Duration = Duration::from_millis(5);

/// The prefix of the error log line the guest SDK writes just before it traps on a panic
/// (`faber_connector_wasm::PANIC_PREFIX`).
const PANIC_PREFIX: &str = "panicked: ";

/// Log lines one instance may write per second; the rest are dropped and counted.
const LOG_LINES_PER_S: u32 = 50;
const LOG_LINE_MAX: usize = 2000;

/// The one engine every wasm connector runs on, with the thread that advances its epoch.
fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).expect("the wasm engine's configuration is valid");
        let ticking = engine.clone();
        std::thread::Builder::new()
            .name("wasm-epoch".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(EPOCH_TICK);
                    ticking.increment_epoch();
                }
            })
            .expect("spawn the wasm epoch thread");
        engine
    })
}

fn bug_hint() -> &'static str {
    "Faber restarts it automatically. If this keeps happening, report it to the connector's author."
}

/// A compiled connector, shared by every part that uses it.
struct Compiled {
    manifest: Manifest,
    pre: bindings::ConnectorPre<Host>,
    limits: Limits,
    signals: Arc<[String]>,
}

/// Makes a [`Definition`] from a component and its manifest. Fails, with what to do, when the
/// file isn't a component, asks for anything outside `faber:`, or lacks the connector exports.
pub fn definition(manifest: Manifest, wasm: &[u8], limits: Limits) -> Result<Definition, Problem> {
    let engine = engine();
    let not_loadable = |why: String| {
        Problem::new(
            format!("{} can't be loaded: {why}", manifest.name),
            "Rebuild the connector against faber:connector@0.1.0 (sdk/rust/connector-wasm) and install it again.",
        )
    };
    let component = Component::new(engine, wasm)
        .map_err(|e| not_loadable(format!("it isn't a WebAssembly component ({e})")))?;
    let forbidden = forbidden_imports(engine, &component);
    if !forbidden.is_empty() {
        return Err(Problem::new(
            format!(
                "{} asks for {}, which Faber doesn't give connectors.",
                manifest.name,
                forbidden.join(", ")
            ),
            "Connectors get bytes from their port and nothing else. Build it for wasm32-unknown-unknown, \
             not with WASI.",
        ));
    }
    let mut linker = Linker::new(engine);
    bindings::Connector::add_to_linker::<Host, HasSelf<Host>>(&mut linker, |h| h)
        .map_err(|e| not_loadable(e.to_string()))?;
    let pre = linker
        .instantiate_pre(&component)
        .and_then(bindings::ConnectorPre::new)
        .map_err(|e| not_loadable(e.to_string()))?;
    let signals = manifest.provides.iter().map(|p| p.name.clone()).collect();
    let compiled = Arc::new(Compiled { manifest: manifest.clone(), pre, limits, signals });
    Ok(Definition::new(manifest, move |settings| compiled.create(settings)).with_kind(ConnectorKind::Wasm))
}

/// The only interface a connector may call.
const CONTEXT: &str = "faber:connector/context@0.1.0";

/// Everything a component imports beyond the connector world: anything outside `faber:`, and
/// any function outside `context` (the capability interfaces it takes types from also declare
/// commands for apps).
fn forbidden_imports(engine: &Engine, component: &Component) -> Vec<String> {
    use wasmtime::component::types::ComponentItem;
    let mut out = Vec::new();
    for (name, item) in component.component_type().imports(engine) {
        if !name.starts_with("faber:") {
            out.push(name.to_string());
            continue;
        }
        match item.ty {
            ComponentItem::ComponentInstance(instance) if name != CONTEXT => {
                for (func, kind) in instance.exports(engine) {
                    if matches!(kind.ty, ComponentItem::ComponentFunc(_)) {
                        out.push(format!("{name}#{func}"));
                    }
                }
            }
            ComponentItem::ComponentInstance(_) | ComponentItem::Type(_) => {}
            _ => out.push(name.to_string()),
        }
    }
    out
}

impl Compiled {
    fn create(&self, settings: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
        let host = Host {
            name: self.manifest.name.clone(),
            id: self.manifest.id.clone(),
            signals: Arc::clone(&self.signals),
            memory: MemoryLimit { max: self.limits.memory_bytes, hit: false },
            events: Vec::new(),
            refused: None,
            panic: None,
            log_window: (Instant::now(), 0, 0),
        };
        let mut store = Store::new(engine(), host);
        store.limiter(|h| &mut h.memory);
        let ticks = ticks(self.limits.call_time);
        let mut c =
            WasmConnector { store, ticks, call_time: self.limits.call_time, instance: None, broken: None };
        store_deadline(&mut c.store, ticks);
        let instance = match self.pre.instantiate(&mut c.store) {
            Ok(i) => i,
            Err(e) => {
                c.broken = Some(c.problem(e, "start"));
                return Ok(Box::new(c));
            }
        };
        let values = convert::settings_to_wit(settings);
        store_deadline(&mut c.store, ticks);
        let result = instance.call_create(&mut c.store, &values);
        c.store.data_mut().events.clear(); // nothing to publish to before the part runs
        match result {
            Ok(Ok(())) => c.instance = Some(instance),
            Ok(Err(e)) => return Err(SettingsError { key: e.key, message: e.message, hint: e.hint }),
            Err(e) => c.broken = Some(c.problem(e, "start")),
        }
        Ok(Box::new(c))
    }
}

/// Epoch ticks for a call that may take `limit`: never less than `limit`, at most a tick more.
fn ticks(limit: Duration) -> u64 {
    (limit.as_micros().div_ceil(EPOCH_TICK.as_micros()) + 1) as u64
}

fn store_deadline(store: &mut Store<Host>, ticks: u64) {
    store.set_epoch_deadline(ticks);
}

/// Keeps one instance under its memory limit, and remembers hitting it.
struct MemoryLimit {
    max: usize,
    hit: bool,
}

impl ResourceLimiter for MemoryLimit {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _: Option<usize>,
    ) -> wasmtime::Result<bool> {
        if desired > self.max {
            self.hit = true;
            return Ok(false);
        }
        Ok(true)
    }

    fn table_growing(&mut self, _current: usize, desired: usize, _: Option<usize>) -> wasmtime::Result<bool> {
        Ok(desired <= 100_000)
    }
}

/// What a connector did during one call, replayed into the part's [`Context`] after it.
enum Event {
    Publish(String, Reading),
    Health(Health),
}

/// The core's side of one instance: what the component's `context` calls reach.
struct Host {
    name: String,
    id: String,
    signals: Arc<[String]>,
    memory: MemoryLimit,
    events: Vec<Event>,
    /// Set when the connector broke a rule during a call; stops the part after it.
    refused: Option<Problem>,
    /// The guest's last panic message, written just before it traps.
    panic: Option<String>,
    /// Start of the current second, lines logged in it, lines dropped in it.
    log_window: (Instant, u32, u32),
}

impl bindings::faber::connector::context::Host for Host {
    fn publish(&mut self, signal: String, reading: wit::Reading) {
        if !self.signals.contains(&signal) {
            tracing::warn!(
                target: "core::wasm",
                "{} published to \"{signal}\", which its manifest doesn't declare; refused.",
                self.id
            );
            self.refused.get_or_insert_with(|| {
                Problem::new(
                    format!(
                        "The connector stopped because of a bug: it published to \"{signal}\", which its manifest \
                         doesn't provide."
                    ),
                    bug_hint(),
                )
            });
            return;
        }
        self.events.push(Event::Publish(signal, convert::reading_from_wit(reading)));
    }

    fn set_health(&mut self, health: wit::Health) {
        self.events.push(Event::Health(convert::health_from_wit(health)));
    }

    fn log(&mut self, level: wit::LogLevel, message: String) {
        if level == wit::LogLevel::Error
            && let Some(text) = message.strip_prefix(PANIC_PREFIX)
        {
            self.panic = Some(text.to_string());
        }
        let (start, lines, dropped) = &mut self.log_window;
        if start.elapsed() >= Duration::from_secs(1) {
            if *dropped > 0 {
                tracing::warn!(target: "core::wasm", "{}: {dropped} log lines dropped (too many)", self.id);
            }
            *start = Instant::now();
            (*lines, *dropped) = (0, 0);
        }
        if *lines >= LOG_LINES_PER_S {
            *dropped += 1;
            return;
        }
        *lines += 1;
        let mut text = message;
        if text.len() > LOG_LINE_MAX {
            let cut = (0..=LOG_LINE_MAX).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
            text.truncate(cut);
            text.push('…');
        }
        log_line(level, &self.id, &text);
    }
}

/// Logs as the part when called on its thread (`part:<name>`), else as the connector.
fn log_line(level: wit::LogLevel, id: &str, text: &str) {
    let thread = std::thread::current();
    let part = thread.name().and_then(|n| n.strip_prefix("part:")).unwrap_or(id);
    match level {
        wit::LogLevel::Debug => tracing::debug!(target: "core::wasm", part, "{text}"),
        wit::LogLevel::Info => tracing::info!(target: "core::wasm", part, "{text}"),
        wit::LogLevel::Warn => tracing::warn!(target: "core::wasm", part, "{text}"),
        wit::LogLevel::Error => tracing::error!(target: "core::wasm", part, "{text}"),
    }
}

// Interfaces the connector world only takes types from: nothing for the host to provide.
impl bindings::faber::connector::types::Host for Host {}
impl bindings::faber::capabilities::types::Host for Host {}
impl bindings::faber::capabilities::laser_scan::Host for Host {}
impl bindings::faber::capabilities::distance::Host for Host {}
impl bindings::faber::capabilities::pose::Host for Host {}
impl bindings::faber::capabilities::position::Host for Host {}

// These interfaces also declare commands for apps. A connector never gets them: `definition`
// refuses any component that imports a function outside `context`. Refused here too, in case.
use bindings::faber::capabilities::types::{CommandError as AppCommandError, LeaseId, Twist2d};

impl bindings::faber::capabilities::motor::Host for Host {
    fn set_velocity(&mut self, _: LeaseId, _: f32) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
    fn set_position(&mut self, _: LeaseId, _: f32) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
    fn set_duty(&mut self, _: LeaseId, _: f32) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
}

impl bindings::faber::capabilities::planar_base::Host for Host {
    fn set_velocity(&mut self, _: LeaseId, _: Twist2d) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
}

impl bindings::faber::capabilities::estop::Host for Host {
    fn engage(&mut self, _: String) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
    fn release(&mut self) -> Result<(), AppCommandError> {
        Err(AppCommandError::BadCommand)
    }
}

/// One part's instance, seen by the runtime as an ordinary [`Connector`].
struct WasmConnector {
    store: Store<Host>,
    ticks: u64,
    call_time: Duration,
    instance: Option<bindings::Connector>,
    /// Why it can't run any more: it trapped, hit a limit or broke a rule. Every later call ends
    /// with this problem, so the runtime restarts it with a fresh instance.
    broken: Option<Problem>,
}

impl WasmConnector {
    /// Calls into the component, then hands what it did to `ctx`. A trap, a limit or a broken
    /// rule ends the call by unwinding with the problem (see the module docs).
    fn call<T>(
        &mut self,
        ctx: &mut dyn Context,
        doing: &str,
        f: impl FnOnce(&bindings::Connector, &mut Store<Host>) -> wasmtime::Result<T>,
    ) -> T {
        let instance = match (&self.broken, &self.instance) {
            (None, Some(i)) => i,
            (Some(p), _) => resume_unwind(Box::new(p.clone())),
            (None, None) => unreachable!("an instance that isn't broken has been created"),
        };
        store_deadline(&mut self.store, self.ticks);
        let result = f(instance, &mut self.store);
        let host = self.store.data_mut();
        for event in host.events.drain(..) {
            match event {
                Event::Publish(signal, reading) => ctx.publish(&signal, reading),
                Event::Health(health) => ctx.set_health(health),
            }
        }
        let problem = match result {
            Ok(value) => match host.refused.take() {
                None => return value,
                Some(p) => p,
            },
            Err(e) => self.problem(e, doing),
        };
        self.broken = Some(problem.clone());
        resume_unwind(Box::new(problem))
    }

    /// Why a call failed, in words for the user.
    fn problem(&self, error: wasmtime::Error, doing: &str) -> Problem {
        let host = self.store.data();
        if host.memory.hit {
            return Problem::new(
                format!(
                    "{} needed more than {} MB of memory and was stopped.",
                    host.name,
                    host.memory.max >> 20
                ),
                bug_hint(),
            );
        }
        if error.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
            return Problem::new(
                format!(
                    "{} took longer than {} ms to {doing} and was stopped.",
                    host.name,
                    self.call_time.as_millis()
                ),
                bug_hint(),
            );
        }
        let what = match &host.panic {
            Some(text) => format!("it panicked ({text})"),
            None => format!("it crashed ({error})"),
        };
        tracing::error!(target: "core::wasm", "{}: {what}", host.id);
        Problem::new(format!("The connector stopped because of a bug: {what}."), bug_hint())
    }
}

impl Connector for WasmConnector {
    fn on_bytes(&mut self, ctx: &mut dyn Context, t_us: Timestamp, bytes: &[u8]) {
        self.call(ctx, "handle data", |c, s| c.call_on_bytes(s, t_us, bytes));
    }

    fn on_tick(&mut self, ctx: &mut dyn Context, now_us: Timestamp) {
        self.call(ctx, "check on itself", |c, s| c.call_on_tick(s, now_us));
    }

    fn command(&mut self, ctx: &mut dyn Context, command: Command) -> Result<Applied, CommandError> {
        let cmd = convert::command_to_wit(command);
        self.call(ctx, "handle a command", |c, s| c.call_command(s, cmd))
            .map(|a| Applied { command: convert::command_from_wit(a.command) })
            .map_err(convert::command_error_from_wit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_gets_at_least_its_time_and_at_most_a_tick_more() {
        let t = ticks(Duration::from_millis(20));
        assert!(Duration::from_millis(20) <= EPOCH_TICK * (t as u32 - 1));
        assert!(EPOCH_TICK * (t as u32 - 1) <= Duration::from_millis(20) + EPOCH_TICK);
    }

    #[test]
    fn the_default_limits_are_the_documented_ones() {
        assert_eq!(
            Limits::default(),
            Limits { memory_bytes: 32 * 1024 * 1024, call_time: Duration::from_millis(20) }
        );
    }
}
