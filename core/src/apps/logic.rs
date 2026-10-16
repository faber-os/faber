//! An app's logic in the sandbox (docs/apps.md §4): one component instance per run, under the
//! same limits as wasm connectors (memory, time per call). Every import goes to the run's
//! [`Session`]; nothing else exists inside: no files, network, environment or clock.

use super::channel::Payload;
use super::session::{DriveError, Session};
use crate::sandbox::app_bindings::faber::app as wit_app;
use crate::sandbox::bindings::faber::capabilities as cap;
use crate::sandbox::bindings::faber::connector::types as wit;
use crate::sandbox::{
    self, Limits, LogLimit, MemoryLimit, PANIC_PREFIX, app_bindings, convert, engine, ticks,
};
use faber_connector::Problem;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Store, Trap};

/// The interfaces an app may call.
const ALLOWED: &[&str] = &[
    "faber:app/roles@0.1.0",
    "faber:app/drive@0.1.0",
    "faber:app/publish@0.1.0",
    "faber:app/state@0.1.0",
    "faber:app/messages@0.1.0",
    "faber:app/log@0.1.0",
];

/// One run of an app's logic, as the supervisor drives it. A failed call is the problem to show;
/// the run is over then.
pub trait Logic: Send {
    fn start(&mut self, now_us: u64) -> Result<(), Problem>;
    fn tick(&mut self, now_us: u64) -> Result<(), Problem>;
    fn stop(&mut self) -> Result<(), Problem>;
    fn session(&mut self) -> &mut Session;
}

/// An app's component, compiled once, instantiated for every run.
pub struct Compiled {
    name: String,
    pre: app_bindings::AppPre<AppHost>,
    limits: Limits,
}

impl std::fmt::Debug for Compiled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Compiled").field("name", &self.name).field("limits", &self.limits).finish()
    }
}

/// Compiles an app's component. Fails, with what to do, when the file isn't a component, asks
/// for anything outside the app world, or lacks its exports.
pub fn compile(name: &str, wasm: &[u8], limits: Limits) -> Result<Compiled, Problem> {
    let engine = engine();
    let not_loadable = |why: String| {
        Problem::new(
            format!("{name}'s logic can't be loaded: {why}"),
            "Rebuild the app against faber:app@0.1.0 (sdk/rust/app-wasm) and install it again.",
        )
    };
    let component = Component::new(engine, wasm)
        .map_err(|e| not_loadable(format!("it isn't a WebAssembly component ({e})")))?;
    let forbidden = sandbox::forbidden_imports(engine, &component, ALLOWED);
    if !forbidden.is_empty() {
        return Err(Problem::new(
            format!("{name} asks for {}, which Faber doesn't give apps.", forbidden.join(", ")),
            "Apps see their roles, their leases, their own signals, a small state, messages with their windows and the log, nothing else. \
             Build it for wasm32-unknown-unknown, not with WASI.",
        ));
    }
    let mut linker = Linker::new(engine);
    app_bindings::App::add_to_linker::<AppHost, HasSelf<AppHost>>(&mut linker, |h| h)
        .map_err(|e| not_loadable(e.to_string()))?;
    let pre = linker
        .instantiate_pre(&component)
        .and_then(app_bindings::AppPre::new)
        .map_err(|e| not_loadable(e.to_string()))?;
    Ok(Compiled { name: name.to_string(), pre, limits })
}

impl Compiled {
    /// A fresh instance for one run, its imports going to `session`.
    pub fn instantiate(&self, session: Session) -> Result<WasmLogic, Problem> {
        let host = AppHost {
            name: self.name.clone(),
            session,
            memory: MemoryLimit::new(self.limits.memory_bytes),
            log: LogLimit::default(),
            panic: None,
        };
        let mut store = Store::new(engine(), host);
        store.limiter(|h| &mut h.memory);
        let ticks = ticks(self.limits.call_time);
        store.set_epoch_deadline(ticks);
        match self.pre.instantiate(&mut store) {
            Ok(instance) => Ok(WasmLogic { store, instance, ticks, limits: self.limits }),
            Err(e) => Err(trap_problem(store.data(), &self.limits, e, "start")),
        }
    }
}

/// The core's side of one instance.
pub struct AppHost {
    name: String,
    session: Session,
    memory: MemoryLimit,
    log: LogLimit,
    /// The guest's last panic message, written just before it traps.
    panic: Option<String>,
}

/// Why a call failed, in words for the user.
fn trap_problem(host: &AppHost, limits: &Limits, error: wasmtime::Error, doing: &str) -> Problem {
    let hint = "Faber restarts it automatically. If this keeps happening, report it to the app's author.";
    if host.memory.hit {
        return Problem::new(
            format!("{} needed more than {} MB of memory and was stopped.", host.name, host.memory.max >> 20),
            hint,
        );
    }
    if error.downcast_ref::<Trap>() == Some(&Trap::Interrupt) {
        return Problem::new(
            format!(
                "{} took longer than {} ms to {doing} and was stopped.",
                host.name,
                limits.call_time.as_millis()
            ),
            hint,
        );
    }
    let what = match &host.panic {
        Some(text) => format!("it panicked ({text})"),
        None => format!("it crashed ({error})"),
    };
    tracing::error!(target: "core::apps", app = host.session.app(), "{what}");
    Problem::new(format!("The app stopped because of a bug: {what}."), hint)
}

/// One run of an app's component.
pub struct WasmLogic {
    store: Store<AppHost>,
    instance: app_bindings::App,
    ticks: u64,
    limits: Limits,
}

impl WasmLogic {
    fn call(
        &mut self,
        doing: &str,
        f: impl FnOnce(&app_bindings::App, &mut Store<AppHost>) -> wasmtime::Result<()>,
    ) -> Result<(), Problem> {
        self.store.set_epoch_deadline(self.ticks);
        let result = f(&self.instance, &mut self.store);
        if let Err(e) = result {
            return Err(trap_problem(self.store.data(), &self.limits, e, doing));
        }
        match self.store.data_mut().session.take_bug() {
            Some(p) => Err(p),
            None => Ok(()),
        }
    }
}

impl Logic for WasmLogic {
    fn start(&mut self, now_us: u64) -> Result<(), Problem> {
        self.call("start", |a, s| a.call_start(s, now_us))
    }

    fn tick(&mut self, now_us: u64) -> Result<(), Problem> {
        self.call("run one tick", |a, s| a.call_tick(s, now_us))
    }

    fn stop(&mut self) -> Result<(), Problem> {
        self.call("stop", |a, s| a.call_stop(s))
    }

    fn session(&mut self) -> &mut Session {
        &mut self.store.data_mut().session
    }
}

// ---- the imports ------------------------------------------------------------------------------

impl wit_app::roles::Host for AppHost {
    fn all(&mut self) -> Vec<wit_app::roles::Role> {
        self.session
            .roles()
            .into_iter()
            .map(|r| wit_app::roles::Role {
                name: r.name,
                capability: r.capability,
                command: r.command,
                parts: r.parts,
            })
            .collect()
    }

    fn latest(&mut self, role: String, index: u32) -> Option<wit::Reading> {
        self.session.latest(&role, index).map(convert::reading_to_wit)
    }

    fn poll(&mut self, role: String, index: u32) -> Option<wit::Reading> {
        self.session.poll(&role, index).map(convert::reading_to_wit)
    }

    fn mount_of(&mut self, role: String, index: u32) -> Option<cap::types::Mount> {
        self.session.mount(&role, index).map(convert::mount_to_wit)
    }
}

fn drive_error(e: DriveError) -> wit_app::drive::DriveError {
    use wit_app::drive::DriveError as W;
    match e {
        DriveError::NoSuchRole => W::NoSuchRole,
        DriveError::Held(who) => W::Held(who),
        DriveError::LeaseRequired => W::LeaseRequired,
        DriveError::LeaseEnded(why) => W::LeaseEnded(why),
        DriveError::EstopEngaged => W::EstopEngaged,
        DriveError::BadCommand(m) => W::BadCommand(m),
        DriveError::PartNotReady(m) => W::PartNotReady(m),
    }
}

impl wit_app::drive::Host for AppHost {
    fn acquire(&mut self, role: String, index: u32) -> Result<(), wit_app::drive::DriveError> {
        self.session.acquire(&role, index).map_err(drive_error)
    }

    fn renew(&mut self, role: String, index: u32) -> Result<(), wit_app::drive::DriveError> {
        self.session.renew(&role, index).map_err(drive_error)
    }

    fn release(&mut self, role: String, index: u32) {
        self.session.release(&role, index)
    }

    fn send(
        &mut self,
        role: String,
        index: u32,
        command: wit::Command,
    ) -> Result<wit::Command, wit_app::drive::DriveError> {
        self.session
            .send(&role, index, convert::command_from_wit(command))
            .map(convert::command_to_wit)
            .map_err(drive_error)
    }
}

impl wit_app::publish::Host for AppHost {
    fn publish(&mut self, name: String, reading: wit::Reading) {
        self.session.publish(&name, convert::reading_from_wit(reading))
    }
}

impl wit_app::state::Host for AppHost {
    fn get(&mut self, key: String) -> Option<String> {
        self.session.state.get(&key)
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        self.session.state.set(&key, &value)
    }

    fn remove(&mut self, key: String) {
        self.session.state.remove(&key)
    }

    fn keys(&mut self) -> Vec<String> {
        self.session.state.keys()
    }
}

fn payload_from_wit(p: wit_app::messages::Payload) -> Payload {
    match p {
        wit_app::messages::Payload::Text(t) => Payload::Text(t),
        wit_app::messages::Payload::Bytes(b) => Payload::Bytes(b),
    }
}

fn payload_to_wit(p: Payload) -> wit_app::messages::Payload {
    match p {
        Payload::Text(t) => wit_app::messages::Payload::Text(t),
        Payload::Bytes(b) => wit_app::messages::Payload::Bytes(b),
    }
}

impl wit_app::messages::Host for AppHost {
    fn poll(&mut self) -> Vec<wit_app::messages::Message> {
        self.session
            .poll_messages()
            .into_iter()
            .map(|m| wit_app::messages::Message {
                window: m.window,
                kind: m.kind,
                payload: payload_to_wit(m.payload),
            })
            .collect()
    }

    fn send(
        &mut self,
        window: Option<u64>,
        kind: String,
        payload: wit_app::messages::Payload,
    ) -> Result<(), String> {
        self.session.send_message(window, &kind, payload_from_wit(payload))
    }
}

impl wit_app::log::Host for AppHost {
    fn log(&mut self, level: wit::LogLevel, message: String) {
        if level == wit::LogLevel::Error
            && let Some(text) = message.strip_prefix(PANIC_PREFIX)
        {
            self.panic = Some(text.to_string());
        }
        let app = self.session.app().to_string();
        let Some(text) = self.log.admit(&app, message) else { return };
        let app = app.as_str();
        match level {
            wit::LogLevel::Debug => tracing::debug!(target: "core::apps", app, "{text}"),
            wit::LogLevel::Info => tracing::info!(target: "core::apps", app, "{text}"),
            wit::LogLevel::Warn => tracing::warn!(target: "core::apps", app, "{text}"),
            wit::LogLevel::Error => tracing::error!(target: "core::apps", app, "{text}"),
        }
    }
}

// Interfaces the app world only takes types from: nothing for the host to provide.
impl sandbox::bindings::faber::connector::types::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::types::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::laser_scan::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::distance::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::pose::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::position::Host for AppHost {}
impl sandbox::bindings::faber::capabilities::battery::Host for AppHost {}

// The capability interfaces also declare commands with lease ids. Apps drive through `drive`
// instead: `compile` refuses a component that imports these, and they refuse here too.
use sandbox::bindings::faber::capabilities::types::{CommandError, LeaseId, Twist2d};

impl sandbox::bindings::faber::capabilities::motor::Host for AppHost {
    fn set_velocity(&mut self, _: LeaseId, _: f32) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
    fn set_position(&mut self, _: LeaseId, _: f32) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
    fn set_duty(&mut self, _: LeaseId, _: f32) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
}

impl sandbox::bindings::faber::capabilities::planar_base::Host for AppHost {
    fn set_velocity(&mut self, _: LeaseId, _: Twist2d) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
}

impl sandbox::bindings::faber::capabilities::estop::Host for AppHost {
    fn engage(&mut self, _: String) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
    fn release(&mut self) -> Result<(), CommandError> {
        Err(CommandError::BadCommand)
    }
}
