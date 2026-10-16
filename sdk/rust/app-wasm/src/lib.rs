//! Builds a Faber app's logic as a sandboxed WebAssembly component (docs/apps.md).
//!
//! An app is plain Rust: a type that implements [`App`]. It sees the machine through a [`Host`]:
//! its roles' readings, leases and commands on the roles it may drive, derived signals it
//! publishes, a small store that outlives restarts, live messages with its windows, and the log.
//! One line turns it into a component:
//!
//! ```ignore
//! #[derive(Default)]
//! struct WallFollower { /* … */ }
//!
//! impl faber_app_wasm::App for WallFollower {
//!     fn tick(&mut self, host: &mut dyn faber_app_wasm::Host, now_us: u64) { /* … */ }
//! }
//!
//! #[cfg(target_arch = "wasm32")]
//! faber_app_wasm::export!(WallFollower);
//! ```
//!
//! Build it with `cargo rustc --lib --crate-type cdylib --release --target wasm32-unknown-unknown`
//! and wrap the module as a component (`faber check` and `faber pack` do both). The component
//! imports only the `faber:app` world: no files, network, environment or clock.
//!
//! The [`Host`] trait is the whole sandbox seen from inside, so an app's logic can be tested on
//! the PC with [`testing::TestHost`], without wasm.

mod convert;
pub mod testing;

#[doc(hidden)]
#[allow(unsafe_code, clippy::all)]
pub mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/capabilities", "../../../wit/connector", "../../../wit/app"],
        world: "faber:app/app@0.1.0",
        pub_export_macro: true,
        generate_all,
        default_bindings_module: "faber_app_wasm::bindings",
    });
}

pub use faber_capabilities::{Command, Mount, Reading};
use std::cell::RefCell;

/// The prefix of the error log line an app writes just before it traps on a panic. The core
/// shows the rest as the reason the app stopped.
pub const PANIC_PREFIX: &str = "panicked: ";

/// One role from the manifest's `[[requires]]`, as the core filled it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    /// e.g. `laser-scan@1`.
    pub capability: String,
    /// The app may drive it (`command = true`).
    pub command: bool,
    /// How many parts fill it (1, more for `count = "1.."` or `"0.."`, maybe 0 for an optional role).
    pub parts: u32,
}

/// Why a lease or a command was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveError {
    /// No role with this name and index that the app may drive.
    NoSuchRole,
    /// Someone else drives the part.
    Held {
        holder: String,
    },
    /// The app holds no lease on the role.
    LeaseRequired,
    /// The lease ended (`taken-over`, `expired`, `estop`, …). Acquire it again to keep going.
    LeaseEnded {
        reason: String,
    },
    EstopEngaged,
    BadCommand(String),
    PartNotReady(String),
}

/// What a message between the app's windows and its logic carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    Text(String),
    Bytes(Vec<u8>),
}

impl Payload {
    /// The text, if it is text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Payload::Text(t) => Some(t),
            Payload::Bytes(_) => None,
        }
    }
}

/// A message from one of the app's windows (`faber.send(kind, data)`), or from the core about a
/// window ([`DRIVE_STARTED`], [`DRIVE_ENDED`]). Never saved: what isn't polled while the logic
/// runs is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The window it is from; answer it with [`Host::send_message`].
    pub window: u64,
    /// What it is, e.g. `drive`.
    pub kind: String,
    pub payload: Payload,
}

/// The core says a window started driving a `drive = "window"` role through the logic.
pub const DRIVE_STARTED: &str = "drive-started";
/// The core says a window's drive ended. The core has ended the logic's lease on the role
/// already (unless another window still drives it), whatever the logic does.
pub const DRIVE_ENDED: &str = "drive-ended";

/// A window's drive through the logic, from a [`DRIVE_STARTED`] or [`DRIVE_ENDED`] message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveEvent {
    Started {
        role: String,
        index: u32,
    },
    /// `reason`: `released`, `stopped` (the user pressed Stop), `window-closed`,
    /// `heartbeat-lost` (the window hung) or `app-stopped`.
    Ended {
        role: String,
        index: u32,
        reason: String,
    },
}

impl Message {
    /// The drive event this message is, if the core sent it.
    pub fn drive_event(&self) -> Option<DriveEvent> {
        let text = self.payload.as_text()?;
        let mut words = text.split(' ');
        let role = words.next()?.to_string();
        let index = words.next()?.parse().ok()?;
        match self.kind.as_str() {
            DRIVE_STARTED => Some(DriveEvent::Started { role, index }),
            DRIVE_ENDED => Some(DriveEvent::Ended { role, index, reason: words.next()?.to_string() }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// The machine, seen from inside an app. Only roles by name: never part names or ports.
pub trait Host {
    /// Every role, in the manifest's order.
    fn roles(&mut self) -> Vec<Role>;
    /// The newest reading of the part at `index` in `role`.
    fn latest(&mut self, role: &str, index: u32) -> Option<Reading>;
    /// Like [`Host::latest`], but only a reading this app hasn't polled before.
    fn poll(&mut self, role: &str, index: u32) -> Option<Reading>;
    /// Where the part at `index` in `role` sits on the machine (x forward, y left; m, rad). Its
    /// readings are in its own frame: apply this to put them in the machine's.
    fn mount(&mut self, role: &str, index: u32) -> Option<Mount>;
    /// Takes (or renews) the lease on a role declared with `command = true`.
    fn acquire(&mut self, role: &str, index: u32) -> Result<(), DriveError>;
    fn renew(&mut self, role: &str, index: u32) -> Result<(), DriveError>;
    fn release(&mut self, role: &str, index: u32);
    /// Sends a command under the app's lease; answers it as applied (clamped).
    fn send(&mut self, role: &str, index: u32, command: Command) -> Result<Command, DriveError>;
    /// Publishes a derived signal declared in `[[publishes]]`, as `app:<id>/<name>`.
    fn publish(&mut self, name: &str, reading: Reading);
    fn state_get(&mut self, key: &str) -> Option<String>;
    /// Fails, saying why, when the store would grow past its limit (64 KB).
    fn state_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn state_remove(&mut self, key: &str);
    fn state_keys(&mut self) -> Vec<String>;
    /// Every message from the app's windows since the last call, oldest first. Call it every tick.
    fn poll_messages(&mut self) -> Vec<Message>;
    /// Sends a message to one window, or to every open window with `None`. At most 4 KB with its
    /// kind and 50 a second per window; fails, saying why, otherwise.
    fn send_message(&mut self, window: Option<u64>, kind: &str, payload: Payload) -> Result<(), String>;
    fn log(&mut self, level: LogLevel, message: &str);
}

/// An app's logic. The core calls `start` once, `tick` at the manifest's `tick-hz`, and `stop`
/// once; the app's leases end after `stop`, whatever it does.
pub trait App {
    fn start(&mut self, _host: &mut dyn Host, _now_us: u64) {}
    fn tick(&mut self, host: &mut dyn Host, now_us: u64);
    fn stop(&mut self, _host: &mut dyn Host) {}
}

// ---- inside the component ---------------------------------------------------------------------

use bindings::faber::app::{drive, log, messages, publish, roles, state};

/// The core, reached through the component's imports.
struct Imports;

impl From<drive::DriveError> for DriveError {
    fn from(e: drive::DriveError) -> Self {
        match e {
            drive::DriveError::NoSuchRole => DriveError::NoSuchRole,
            drive::DriveError::Held(holder) => DriveError::Held { holder },
            drive::DriveError::LeaseRequired => DriveError::LeaseRequired,
            drive::DriveError::LeaseEnded(reason) => DriveError::LeaseEnded { reason },
            drive::DriveError::EstopEngaged => DriveError::EstopEngaged,
            drive::DriveError::BadCommand(m) => DriveError::BadCommand(m),
            drive::DriveError::PartNotReady(m) => DriveError::PartNotReady(m),
        }
    }
}

fn level(l: LogLevel) -> bindings::faber::connector::types::LogLevel {
    use bindings::faber::connector::types::LogLevel as W;
    match l {
        LogLevel::Debug => W::Debug,
        LogLevel::Info => W::Info,
        LogLevel::Warn => W::Warn,
        LogLevel::Error => W::Error,
    }
}

impl Host for Imports {
    fn roles(&mut self) -> Vec<Role> {
        roles::all()
            .into_iter()
            .map(|r| Role { name: r.name, capability: r.capability, command: r.command, parts: r.parts })
            .collect()
    }

    fn latest(&mut self, role: &str, index: u32) -> Option<Reading> {
        roles::latest(role, index).map(convert::reading_from_wit)
    }

    fn poll(&mut self, role: &str, index: u32) -> Option<Reading> {
        roles::poll(role, index).map(convert::reading_from_wit)
    }

    fn mount(&mut self, role: &str, index: u32) -> Option<Mount> {
        roles::mount_of(role, index).map(convert::mount_from_wit)
    }

    fn acquire(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        drive::acquire(role, index).map_err(Into::into)
    }

    fn renew(&mut self, role: &str, index: u32) -> Result<(), DriveError> {
        drive::renew(role, index).map_err(Into::into)
    }

    fn release(&mut self, role: &str, index: u32) {
        drive::release(role, index)
    }

    fn send(&mut self, role: &str, index: u32, command: Command) -> Result<Command, DriveError> {
        drive::send(role, index, convert::command_to_wit(command))
            .map(convert::command_from_wit)
            .map_err(Into::into)
    }

    fn publish(&mut self, name: &str, reading: Reading) {
        publish::publish(name, &convert::reading_to_wit(reading))
    }

    fn state_get(&mut self, key: &str) -> Option<String> {
        state::get(key)
    }

    fn state_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        state::set(key, value)
    }

    fn state_remove(&mut self, key: &str) {
        state::remove(key)
    }

    fn state_keys(&mut self) -> Vec<String> {
        state::keys()
    }

    fn poll_messages(&mut self) -> Vec<Message> {
        messages::poll()
            .into_iter()
            .map(|m| Message {
                window: m.window,
                kind: m.kind,
                payload: match m.payload {
                    messages::Payload::Text(t) => Payload::Text(t),
                    messages::Payload::Bytes(b) => Payload::Bytes(b),
                },
            })
            .collect()
    }

    fn send_message(&mut self, window: Option<u64>, kind: &str, payload: Payload) -> Result<(), String> {
        let payload = match payload {
            Payload::Text(t) => messages::Payload::Text(t),
            Payload::Bytes(b) => messages::Payload::Bytes(b),
        };
        messages::send(window, kind, &payload)
    }

    fn log(&mut self, l: LogLevel, message: &str) {
        log::log(level(l), message)
    }
}

thread_local! {
    /// The one app this instance runs (the core makes one instance per app).
    static APP: RefCell<Option<Box<dyn App>>> = const { RefCell::new(None) };
}

/// Logs a panic's message to the core before the component traps, so the app's status can say
/// what went wrong.
fn report_panics() {
    std::panic::set_hook(Box::new(|info| {
        let text = info.payload_as_str().unwrap_or("unknown panic");
        log::log(level(LogLevel::Error), &format!("{PANIC_PREFIX}{text}"));
    }));
}

fn with_app(f: impl FnOnce(&mut dyn App, &mut dyn Host)) {
    APP.with_borrow_mut(|a| {
        if let Some(app) = a.as_mut() {
            f(app.as_mut(), &mut Imports)
        }
    })
}

#[doc(hidden)]
pub fn start(make: fn() -> Box<dyn App>, now_us: u64) {
    report_panics();
    APP.set(Some(make()));
    with_app(|app, host| app.start(host, now_us));
}

#[doc(hidden)]
pub fn tick(now_us: u64) {
    with_app(|app, host| app.tick(host, now_us));
}

#[doc(hidden)]
pub fn stop() {
    with_app(|app, host| app.stop(host));
    APP.set(None);
}

/// Exports an app as a component. `$app` is a type that implements [`App`] and `Default`.
#[macro_export]
macro_rules! export {
    ($app:ty) => {
        struct FaberApp;

        impl $crate::bindings::Guest for FaberApp {
            fn start(now_us: u64) {
                $crate::start(|| ::std::boxed::Box::new(<$app as ::std::default::Default>::default()), now_us)
            }

            fn tick(now_us: u64) {
                $crate::tick(now_us)
            }

            fn stop() {
                $crate::stop()
            }
        }

        $crate::bindings::export!(FaberApp with_types_in $crate::bindings);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(kind: &str, text: &str) -> Message {
        Message { window: 3, kind: kind.into(), payload: Payload::Text(text.into()) }
    }

    #[test]
    fn the_cores_drive_messages_are_read() {
        assert_eq!(
            msg(DRIVE_STARTED, "base 0").drive_event(),
            Some(DriveEvent::Started { role: "base".into(), index: 0 })
        );
        assert_eq!(
            msg(DRIVE_ENDED, "base 1 heartbeat-lost").drive_event(),
            Some(DriveEvent::Ended { role: "base".into(), index: 1, reason: "heartbeat-lost".into() })
        );
        assert_eq!(msg("drive", "base 0").drive_event(), None, "a window's own message");
        assert_eq!(msg(DRIVE_ENDED, "base x stopped").drive_event(), None);
        let bytes = Message { window: 1, kind: DRIVE_ENDED.into(), payload: Payload::Bytes(vec![1]) };
        assert_eq!(bytes.drive_event(), None);
    }
}
