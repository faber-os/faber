//! The simulated world as the machine sees it (docs/simulation.md, section 7): which world is
//! loaded (`[world]` in parts.toml, which the machine follows like everything else in it),
//! switching it, putting the machine back at its start, and what the API shows of it. The
//! world itself runs in the world engine (`simparts`).

use super::{ErrorKind, Machine, MachineError, State, refuse_if_config_broken};
use crate::config::{self, ConfigError, PartsConfig, Simulate, Source, Start, WorldConfig};
use crate::sim::world::{Footprint, Pose};
use crate::sim::worlds::{self, Prebuilt, WorldInfo};
use crate::simparts::WorldState;

/// The world simulated parts live in when parts.toml has no `[world]`.
pub const DEFAULT_WORLD: &str = "empty";

/// The current world, for `GET /api/v0/world`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldView {
    pub world: Prebuilt,
    /// Where the machine starts: `[world] start`, or the world's own start.
    pub start: Pose,
    /// The world base's footprint; `None` while no part is the world's base.
    pub footprint: Option<Footprint>,
}

/// The world was switched (`world-changed` on the `world` WebSocket topic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldChanged {
    pub name: String,
}

/// What the machine last loaded into the world engine.
#[derive(Default)]
pub(super) struct Loaded {
    /// The `[world]` it was loaded from; `None` until the first load.
    config: Option<Option<WorldConfig>>,
    /// "No `[world]`, so the empty world" has been logged for this config.
    told_default: bool,
}

/// The prebuilt worlds' names.
fn known() -> Vec<&'static str> {
    worlds::FILES.iter().map(|(name, _)| *name).collect()
}

/// Reads parts.toml, including whether its `[world]` names a world that exists.
pub(super) fn read_config(text: &str) -> Result<PartsConfig, ConfigError> {
    let config = config::parse(text)?;
    config::check_world(text, &config, &known())?;
    Ok(config)
}

/// The world and start a `[world]` table asks for.
fn resolve(wanted: Option<&WorldConfig>) -> Option<(Prebuilt, Pose)> {
    let name = wanted.map_or(DEFAULT_WORLD, |w| w.name.as_str());
    let world = worlds::get(name)?;
    let start = wanted.and_then(|w| w.start).map_or(world.start, |s| Pose::new(s.x, s.y, s.yaw));
    Some((world, start))
}

fn unknown_world(name: &str) -> MachineError {
    MachineError::new(
        ErrorKind::BadRequest,
        "unknown-world",
        format!("There is no world called \"{name}\"."),
        format!("Pick one of the prebuilt worlds: {}.", known().join(", ")),
    )
}

impl Machine {
    /// Every prebuilt world, in menu order.
    pub fn worlds(&self) -> Vec<WorldInfo> {
        worlds::list()
    }

    /// The world simulated parts are in now.
    pub fn world(&self) -> WorldView {
        let wanted = self.state.lock().config.world.clone();
        let (world, start) =
            resolve(wanted.as_ref()).or_else(|| resolve(None)).expect("the default world is built in");
        WorldView { world, start, footprint: self.world.state().footprint }
    }

    /// The machine in the world now: pose, velocity, bump and the moving obstacles.
    pub fn world_state(&self) -> WorldState {
        self.world.state()
    }

    /// Fires whenever the world is switched.
    pub fn world_events(&self) -> tokio::sync::broadcast::Receiver<WorldChanged> {
        self.world_events.subscribe()
    }

    /// Picks the world (and the machine's start in it, or the world's own) by writing `[world]`
    /// in parts.toml, keeping its comments. Switching rebuilds the world with the machine at
    /// rest at its start.
    pub fn set_world(&self, name: &str, start: Option<Pose>) -> Result<WorldView, MachineError> {
        let world = worlds::get(name).ok_or_else(|| unknown_world(name))?;
        if let Some(s) = start {
            let b = world.geometry.bounds;
            if ![s.x, s.y, s.yaw].iter().all(|v| v.is_finite()) || !b.contains(s.position()) {
                return Err(MachineError::new(
                    ErrorKind::BadRequest,
                    "start-outside-world",
                    format!(
                        "The start ({}, {}) lies outside {}, which spans x {}–{} m and y {}–{} m.",
                        s.x, s.y, world.title, b.min.x, b.max.x, b.min.y, b.max.y
                    ),
                    "Pick a start inside the world, or leave out start to use the world's own.",
                ));
            }
        }
        let mut st = self.state.lock();
        refuse_if_config_broken(&st)?;
        let wanted = WorldConfig {
            name: name.to_string(),
            start: start.map(|p| Start { x: p.x, y: p.y, yaw: p.yaw }),
        };
        let text = config::set_world(&st.text, &wanted).map_err(super::config_write_error)?;
        self.save(&mut st, text)?;
        self.reconcile(&mut st);
        drop(st);
        Ok(self.world())
    }

    /// Puts the machine back at its start, at rest.
    pub fn reset_world(&self) {
        self.world.reset();
        tracing::info!(target: "core::world", "the machine is back at its start");
    }

    /// Loads `[world]` into the world engine when it changed (on start, after an edit of
    /// parts.toml, or through [`set_world`](Machine::set_world)): the world is rebuilt and the
    /// machine put at rest at its start.
    pub(super) fn sync_world(&self, st: &mut State) {
        let wanted = st.config.world.clone();
        if st.world.config.as_ref() == Some(&wanted) {
            return;
        }
        let Some((world, start)) = resolve(wanted.as_ref()) else {
            // read_config refuses unknown worlds, so this is a bug, not a user's mistake.
            tracing::error!(target: "core::world", "parts.toml names a world that doesn't exist: {wanted:?}");
            return;
        };
        self.world.load(world.world(), start);
        let switched = st.world.config.is_some();
        st.world = Loaded { config: Some(wanted), told_default: false };
        if switched {
            tracing::info!(target: "core::world", "the world is now {} ({})", world.title, world.name);
            let _ = self.world_events.send(WorldChanged { name: world.name });
        }
    }

    /// Says once when world parts run without a `[world]`, so the user knows where they are.
    pub(super) fn tell_default_world(&self, st: &mut State) {
        if st.config.world.is_some() || st.world.told_default {
            return;
        }
        let world_parts = st.config.parts.values().any(|p| p.source == Source::Simulated(Simulate::World));
        if world_parts {
            tracing::info!(
                target: "core::world",
                "parts.toml has no [world], so the world parts are in the \"{DEFAULT_WORLD}\" world. Pick one in the World window, or add [world] with name = \"apartment\" (say) to parts.toml."
            );
            st.world.told_default = true;
        }
    }
}
