//! Builds a Faber connector as a sandboxed WebAssembly component (docs/packages.md §3).
//!
//! A connector author writes the same Rust as for a built-in connector: a [`Definition`] whose
//! `create` makes a [`Connector`]. One line turns it into a component:
//!
//! ```ignore
//! #[cfg(target_arch = "wasm32")]
//! faber_connector_wasm::export!(definition);
//! ```
//!
//! Build it with `cargo rustc --lib --crate-type cdylib --release --target wasm32-unknown-unknown`
//! and wrap the module as a component (`wasm-tools component new`). The component imports only
//! `faber:connector/context`: no files, network, environment or clock.
//!
//! This crate is the guest half; the core's `wasm` module is the host half. Both convert between
//! the WIT types and the Rust types in `faber-capabilities`, so a reading is the same value on
//! both sides of the sandbox.

mod convert;

#[doc(hidden)]
#[allow(unsafe_code, clippy::all)]
pub mod bindings {
    wit_bindgen::generate!({
        path: ["../../../wit/capabilities", "../../../wit/connector"],
        world: "faber:connector/connector@0.1.0",
        pub_export_macro: true,
        generate_all,
        default_bindings_module: "faber_connector_wasm::bindings",
    });
}

use bindings::faber::connector::context;
use bindings::faber::connector::types as wit;
use faber_capabilities::{Reading, Timestamp};
use faber_connector::{Connector, Context, Definition, Health};
use std::cell::RefCell;

/// The prefix of the error log line a connector writes just before it traps on a panic. The core
/// shows the rest as the reason the part stopped.
pub const PANIC_PREFIX: &str = "panicked: ";

thread_local! {
    /// The one connector this instance runs (the core makes one instance per part).
    static CONNECTOR: RefCell<Option<Box<dyn Connector>>> = const { RefCell::new(None) };
}

/// The core, seen from inside the sandbox.
struct Host;

impl Context for Host {
    fn publish(&mut self, name: &str, reading: Reading) {
        context::publish(name, &convert::reading_to_wit(reading));
    }

    fn set_health(&mut self, health: Health) {
        context::set_health(&convert::health_to_wit(health));
    }
}

/// Logs a panic's message to the core before the component traps, so the part's problem can say
/// what went wrong.
fn report_panics() {
    std::panic::set_hook(Box::new(|info| {
        let text = info.payload_as_str().unwrap_or("unknown panic");
        context::log(wit::LogLevel::Error, &format!("{PANIC_PREFIX}{text}"));
    }));
}

fn with_connector<T>(f: impl FnOnce(&mut dyn Connector) -> T) -> T {
    CONNECTOR.with_borrow_mut(|c| f(c.as_mut().expect("the core calls create first").as_mut()))
}

#[doc(hidden)]
pub fn create(
    definition: fn() -> Definition,
    settings: Vec<(String, wit::SettingValue)>,
) -> Result<(), wit::SettingsError> {
    report_panics();
    let settings = convert::settings_from_wit(settings);
    let connector = (definition().create)(&settings).map_err(convert::settings_error_to_wit)?;
    CONNECTOR.set(Some(connector));
    Ok(())
}

#[doc(hidden)]
pub fn on_bytes(t_us: Timestamp, bytes: &[u8]) {
    with_connector(|c| c.on_bytes(&mut Host, t_us, bytes));
}

#[doc(hidden)]
pub fn on_tick(now_us: Timestamp) {
    with_connector(|c| c.on_tick(&mut Host, now_us));
}

#[doc(hidden)]
pub fn command(cmd: wit::Command) -> Result<wit::Applied, wit::CommandError> {
    let cmd = convert::command_from_wit(cmd);
    with_connector(|c| c.command(&mut Host, cmd))
        .map(|a| wit::Applied { command: convert::command_to_wit(a.command) })
        .map_err(convert::command_error_to_wit)
}

/// Exports a connector as a component. `$definition` is a `fn() -> Definition`, the same function
/// that offers the connector as a built-in.
#[macro_export]
macro_rules! export {
    ($definition:path) => {
        struct FaberConnector;

        impl $crate::bindings::Guest for FaberConnector {
            fn create(
                settings: ::std::vec::Vec<(::std::string::String, $crate::bindings::SettingValue)>,
            ) -> ::std::result::Result<(), $crate::bindings::SettingsError> {
                $crate::create($definition, settings)
            }

            fn on_bytes(t_us: u64, bytes: ::std::vec::Vec<u8>) {
                $crate::on_bytes(t_us, &bytes)
            }

            fn on_tick(now_us: u64) {
                $crate::on_tick(now_us)
            }

            fn command(
                cmd: $crate::bindings::Command,
            ) -> ::std::result::Result<$crate::bindings::Applied, $crate::bindings::CommandError> {
                $crate::command(cmd)
            }
        }

        $crate::bindings::export!(FaberConnector with_types_in $crate::bindings);
    };
}
