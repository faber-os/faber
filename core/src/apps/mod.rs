//! Layer 3: apps (docs/apps.md). An app uses capabilities to do a job; it never talks to
//! hardware or connectors, only to the parts the user picked for its roles (D04).
//!
//! - [`manifest`]: its `faber.toml`, read strictly.
//! - [`load_installed`] / [`load_folder`]: the apps in `<data>/apps/`, their logic compiled.
//! - The logic runs in the same sandbox as wasm connectors (`sandbox`), one instance per run,
//!   on its own thread ticked at the manifest's `tick-hz` ([`start`]), supervised like a part:
//!   a crash ends its leases and restarts it with backoff, until it crashes too often.
//! - Everything it can do goes through a [`Session`]: its roles by name, leases held as
//!   `app:<id>` through a [`Driver`] (the machine), its derived signals `app:<id>/<name>`, a
//!   small state and the log.

mod installed;
mod logic;
pub mod manifest;
mod session;
mod state;
mod supervisor;

pub use installed::{AppDefinition, load_folder, load_installed};
pub use logic::{Compiled, Logic, WasmLogic, compile};
pub use session::{BoundPart, BoundRole, DriveError, Driver, RoleView, Session, holder};
pub use state::{MAX_BYTES as STATE_MAX_BYTES, SharedState, StateChange, StateStore};
pub use supervisor::{AppHandle, AppStatus, MakeLogic, RunDeps, RunSpec, StatusBoard, Timing, start};

use crate::router::Router;
use std::sync::Arc;

/// The file an app's state is saved in, in its folder.
pub const STATE_FILE: &str = "state.toml";

impl AppDefinition {
    /// Where this app's state is saved.
    pub fn state_file(&self) -> std::path::PathBuf {
        self.folder.join(STATE_FILE)
    }

    /// What runs this app with these roles: a fresh instance and session for every run, all with
    /// the app's one shared `state`. `None` for a UI-only app (it has no logic to run).
    pub fn run_spec(
        &self,
        roles: Vec<BoundRole>,
        router: Arc<Router>,
        driver: Arc<dyn Driver>,
        state: Arc<SharedState>,
    ) -> Option<RunSpec> {
        let compiled = Arc::clone(self.logic.as_ref()?);
        let m = &self.manifest;
        let (id, publishes) = (m.id.clone(), m.publishes.clone());
        let make: MakeLogic = Box::new(move || {
            let session = Session::new(
                &id,
                roles.clone(),
                publishes.clone(),
                Arc::clone(&router),
                Arc::clone(&driver),
                Arc::clone(&state),
            );
            Ok(Box::new(compiled.instantiate(session)?) as Box<dyn Logic>)
        });
        Some(RunSpec { id: m.id.clone(), tick_hz: m.tick_hz, publishes: m.publishes.clone(), make })
    }
}
