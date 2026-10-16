//! The machine's safety wiring (D12, docs/simulation.md, sections 5 and 6): leases, commands
//! and the e-stop. The lease table (`leases`) and the e-stop state (`estop`) are pure; this is
//! where they meet the running parts:
//! - every command is checked against the e-stop and the lease, clamped to the part's info, and
//!   renews the lease;
//! - whenever a lease ends, for any reason, its parts get their safe state and `lease-ended` is
//!   announced;
//! - a sweeper ends expired leases (and leases on parts that failed) within 50 ms;
//! - a bridge's sub-parts (`mega.left`) take leases and commands like any part; their commands,
//!   safe states and the e-stop go to the board through the bridge.

use super::{ErrorKind, Machine, MachineError, Running, State, bridge_handle};
use crate::bridge::{BridgeEvent, BridgeHandle};
use crate::leases::{EndReason, Ended, Lease, LeaseError, Request};
use crate::names::PartName;
use crate::runtime::PartStatus;
use crate::simulation::{Pattern, PatternError, SimulatedPart};
use faber_capabilities::{Command, Info, Refused, estop};
use faber_connector::{Applied, CommandError, Problem, Provide};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// How often the sweeper looks for expired leases. Well inside the 50 ms the design promises.
pub(super) const SWEEP_EVERY: Duration = Duration::from_millis(10);

/// What runs a part: the runtime (a real part on its port), a simulation, or a bridge (a
/// FaberLink board on its port, which runs its sub-parts too).
pub(super) enum Runner {
    Real(crate::runtime::PartHandle),
    Simulated(Box<dyn SimulatedPart>),
    Bridge(BridgeHandle),
}

impl Runner {
    fn command(&mut self, command: Command) -> Result<Applied, CommandError> {
        match self {
            Runner::Real(h) => h.command(command),
            Runner::Simulated(s) => s.command(command),
            // The bridge itself takes no commands; its sub-parts do.
            Runner::Bridge(_) => Err(CommandError::Unsupported),
        }
    }

    pub(super) fn stop(self) {
        match self {
            Runner::Real(h) => h.stop(),
            Runner::Simulated(s) => drop(s),
            Runner::Bridge(h) => h.stop(),
        }
    }
}

/// The bridge a sub-part is on, while the bridge runs.
fn bridge_of<'a>(st: &'a State, part: &PartName) -> Option<&'a BridgeHandle> {
    bridge_handle(st, &part.bridge()?)
}

/// What a part provides now: its own (with its settings), or a sub-part's, from its board.
pub(super) fn provides_of(st: &State, part: &PartName) -> Vec<Provide> {
    if part.is_sub() {
        return bridge_of(st, part)
            .and_then(|b| b.sub_part(part))
            .and_then(|s| s.provide)
            .into_iter()
            .collect();
    }
    st.running.get(part).map(|r| r.provides.clone()).unwrap_or_default()
}

/// A lease was granted or ended (the `leases` WebSocket topic).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaseEvent {
    Granted(Lease),
    Ended(Ended),
}

/// The answer to a command: what was applied, after clamping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CommandReply {
    pub applied: Command,
    /// The command was beyond the part's limits and was clamped to them.
    pub clamped: bool,
    /// The lease's new deadline (µs, core clock).
    pub deadline_us: u64,
}

/// The e-stop as the API shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EstopView {
    /// The change that engaged it, while it is engaged.
    pub engaged: Option<estop::Reading>,
    pub history: Vec<estop::Reading>,
}

fn err(kind: ErrorKind, code: &'static str, message: String, hint: &str) -> MachineError {
    MachineError::new(kind, code, message, hint)
}

fn estop_engaged(st: &State) -> MachineError {
    let why = st.estop.engaged().map(|r| format!(" ({})", r.reason)).unwrap_or_default();
    err(
        ErrorKind::Locked,
        "estop-engaged",
        format!("The e-stop is engaged{why}, so nothing may move."),
        "Release the e-stop (the Stop button on the taskbar, or POST /api/v0/estop/release) when it is safe, then try again.",
    )
}

/// Refuses a part that is paused, or is on a paused board: nothing runs it until it is resumed.
fn paused(st: &State, part: &str) -> Option<MachineError> {
    let name = PartName::parse_any(part).ok()?;
    let top = name.bridge().unwrap_or_else(|| name.clone());
    st.config.parts.get(&top).filter(|c| c.paused)?;
    let message = if top == name {
        format!("\"{name}\" is paused.")
    } else {
        format!("\"{name}\" is on the board \"{top}\", which is paused.")
    };
    Some(err(
        ErrorKind::Conflict,
        "part-paused",
        message,
        &format!("Resume \"{top}\" in Parts first (POST /api/v0/parts/{top}/resume)."),
    ))
}

fn unknown_part(name: &str) -> MachineError {
    err(
        ErrorKind::NotFound,
        "unknown-part",
        format!("There is no part called \"{name}\"."),
        "Check the name in the parts list.",
    )
}

fn reason_words(r: EndReason) -> &'static str {
    match r {
        EndReason::Released => "it was released",
        EndReason::Expired => "it expired: no command or renewal arrived in time",
        EndReason::Estop => "the e-stop was engaged",
        EndReason::TakenOver => "someone took over",
        EndReason::PartRemoved => "the part was removed",
        EndReason::PartRestarted => "the part was restarted",
        EndReason::PartFailed => "the part stopped working",
        EndReason::PartPaused => "the part was paused",
        EndReason::Shutdown => "Faber stopped",
    }
}

impl Machine {
    /// Fires on every lease granted or ended.
    pub fn lease_events(&self) -> tokio::sync::broadcast::Receiver<LeaseEvent> {
        self.lease_events.subscribe()
    }

    /// Every notice from now on: a battery's level changing (docs/capabilities.md).
    pub fn notices(&self) -> tokio::sync::broadcast::Receiver<crate::notices::Notice> {
        self.notices.subscribe()
    }

    /// Fires on every engage and release of the e-stop. None is lost (every-message).
    pub fn estop_events(&self) -> tokio::sync::broadcast::Receiver<estop::Reading> {
        self.estop_events.subscribe()
    }

    /// Every live lease, by part.
    pub fn leases(&self) -> Vec<Lease> {
        self.state.lock().leases.active()
    }

    pub fn estop(&self) -> EstopView {
        let st = self.state.lock();
        EstopView { engaged: st.estop.engaged().cloned(), history: st.estop.history().cloned().collect() }
    }

    /// A part in parts.toml, or a sub-part its bridge's board has (or parts.toml names).
    pub(super) fn known_part(st: &State, name: &str) -> Result<PartName, MachineError> {
        let exists = |n: &PartName| match n.bridge() {
            None => st.config.parts.contains_key(n),
            Some(b) => {
                st.config.parts.get(&b).is_some_and(|c| c.parts.contains_key(n.local()))
                    || bridge_of(st, n).and_then(|h| h.sub_part(n)).is_some()
                    || st.paused_boards.get(&b).is_some_and(|had| had.contains(n))
            }
        };
        PartName::parse_any(name).ok().filter(exists).ok_or_else(|| unknown_part(name))
    }

    /// Hands a command to whatever runs the part: its runner, or its bridge.
    fn apply(st: &mut State, part: &PartName, command: Command) -> Result<Applied, CommandError> {
        if part.is_sub() {
            return match bridge_of(st, part) {
                Some(b) => b.command(part, command),
                None => Err(CommandError::NotReady(Problem::new(
                    format!("\"{part}\" isn't running: its bridge isn't."),
                    "Check the bridge's state in Parts.",
                ))),
            };
        }
        match st.running.get_mut(part).and_then(|r| r.runner.as_mut()) {
            Some(r) => r.command(command),
            None => Err(CommandError::NotReady(Problem::new(
                format!("\"{part}\" isn't running."),
                "Check its state in Parts.",
            ))),
        }
    }

    /// Grants `holder` the lease on `part`, or says who holds it. `take_over` ends the lease in
    /// the way (an operator taking over; its holder hears `taken-over`).
    pub fn request_lease(&self, part: &str, holder: &str, take_over: bool) -> Result<Lease, MachineError> {
        let mut st = self.state.lock();
        if let Some(e) = paused(&st, part) {
            return Err(e);
        }
        let name = Self::known_part(&st, part)?;
        if st.estop.is_engaged() {
            return Err(estop_engaged(&st));
        }
        let holder = holder.trim();
        if holder.is_empty() {
            return Err(err(
                ErrorKind::BadRequest,
                "bad-request",
                "A lease needs a holder: who is driving.".into(),
                "Send {\"holder\": \"…\"} with a name people recognise, e.g. the program's name.",
            ));
        }
        let leased = provides_of(&st, &name).into_iter().find_map(|p| Some((p.capability.lease()?, p.info)));
        let Some((policy, info)) = leased else {
            return Err(err(
                ErrorKind::BadRequest,
                "not-leasable",
                format!("\"{name}\" has nothing that moves, so it takes no lease."),
                "Leases are for actuators such as motors and drive bases.",
            ));
        };
        let locks = info.as_ref().map(Info::built_from).unwrap_or_default();
        let locks = locks.iter().filter_map(|n| PartName::parse_any(n).ok()).collect();
        let now = self.clock.now_us();
        let req = Request {
            part: name.clone(),
            holder: holder.into(),
            duration_us: policy.duration_us,
            locks,
            take_over,
        };
        match st.leases.request(req, now) {
            Ok((lease, ended)) => {
                self.finish(&mut st, ended);
                tracing::info!(target: "core::leases", "{} now drives {}", lease.holder, lease.part);
                let _ = self.lease_events.send(LeaseEvent::Granted(lease.clone()));
                Ok(lease)
            }
            Err(LeaseError::Held { part, holder, through }) => {
                let message = match through {
                    Some(base) => format!("\"{part}\" is part of \"{base}\", which \"{holder}\" is driving."),
                    None if part == name => format!("\"{holder}\" is driving \"{name}\"."),
                    None => format!("\"{name}\" is built from \"{part}\", which \"{holder}\" is driving."),
                };
                Err(err(
                    ErrorKind::Conflict,
                    "lease-held",
                    message,
                    "Wait until they are done, or take over (operators only) with {\"takeOver\": true}.",
                ))
            }
            Err(LeaseError::Required | LeaseError::Ended(_)) => unreachable!("request never answers these"),
        }
    }

    /// Keeps a lease alive without sending a command.
    pub fn renew_lease(&self, part: &str, lease: &str) -> Result<Lease, MachineError> {
        let mut st = self.state.lock();
        let name = Self::known_part(&st, part)?;
        let now = self.clock.now_us();
        st.leases.renew(&name, lease, now).map_err(|e| lease_error(&st, &name, e))
    }

    /// The holder is done: the part gets its safe state.
    pub fn release_lease(&self, part: &str, lease: &str) -> Result<(), MachineError> {
        let mut st = self.state.lock();
        let name = Self::known_part(&st, part)?;
        let ended = st.leases.release(&name, lease).map_err(|e| lease_error(&st, &name, e))?;
        self.finish(&mut st, vec![ended]);
        Ok(())
    }

    /// Checks the e-stop and the lease, clamps the command to the part's info, applies it and
    /// renews the lease.
    pub fn command(&self, part: &str, lease: &str, command: Command) -> Result<CommandReply, MachineError> {
        let mut st = self.state.lock();
        let name = Self::known_part(&st, part)?;
        if st.estop.is_engaged() {
            return Err(estop_engaged(&st));
        }
        let provides = provides_of(&st, &name);
        let provide = provides.iter().find(|p| p.capability == command.capability()).cloned();
        let Some(provide) = provide else {
            let has: Vec<_> = provides.iter().map(|p| p.capability.id()).collect();
            return Err(err(
                ErrorKind::BadRequest,
                "bad-command",
                format!("\"{name}\" doesn't take {} commands.", command.capability().id()),
                &match has.as_slice() {
                    [] => "Check that the part is set up; Parts shows what it provides.".to_string(),
                    _ => format!("It provides {}. Send a command for one of those.", has.join(", ")),
                },
            ));
        };
        let now = self.clock.now_us();
        st.leases.check(&name, lease, now).map_err(|e| lease_error(&st, &name, e))?;
        let clamped = match &provide.info {
            Some(info) => command.clamp(info).map_err(|r| refused(&name, r))?,
            // No info: only the limits every part of the capability has (a duty is 0–1).
            None => command.clamp_bare().map_err(|r| refused(&name, r))?,
        };
        let applied = Self::apply(&mut st, &name, clamped.command).map_err(|e| match e {
            CommandError::Unsupported => err(
                ErrorKind::BadRequest,
                "bad-command",
                format!("\"{name}\" doesn't take {} {} commands.", command.capability().id(), command.kind()),
                "Its info says which commands it takes.",
            ),
            CommandError::NotReady(p) => err(ErrorKind::Conflict, "part-not-ready", p.message, &p.hint),
        })?;
        let lease = st.leases.renew(&name, lease, now).expect("checked above, under the same lock");
        Ok(CommandReply {
            applied: applied.command,
            clamped: clamped.clamped,
            deadline_us: lease.deadline_us,
        })
    }

    /// Engages the e-stop: always allowed, for anyone. Every lease ends, every actuator gets its
    /// safe state, and new leases and commands are refused until it is released.
    pub fn engage_estop(&self, source: estop::Source, reason: &str) -> EstopView {
        let mut st = self.state.lock();
        let now = self.clock.now_us();
        let reason = if reason.trim().is_empty() { "No reason given" } else { reason.trim() };
        if let Some(change) = st.estop.engage(source, reason, now) {
            tracing::warn!(target: "core::estop", "e-stop engaged: {reason}");
            let _ = self.estop_events.send(change);
        }
        let ended = st.leases.revoke_all(EndReason::Estop);
        self.finish(&mut st, ended);
        let names: Vec<PartName> = st.running.keys().cloned().collect();
        for name in names {
            self.make_safe(&mut st, &name);
        }
        // Every board hears it at once (ESTOP and SAFE for everything) and in every heartbeat.
        let level = st.estop.engaged().map(|r| (r.source, r.reason.clone()));
        for b in st.running.values().filter_map(|r| r.runner.as_ref()) {
            if let Runner::Bridge(h) = b {
                h.set_estop(level.clone());
            }
        }
        EstopView { engaged: st.estop.engaged().cloned(), history: st.estop.history().cloned().collect() }
    }

    /// Releases the e-stop, unless a board's own stop button is still latched: that is reset on
    /// the board first (docs/faberlink.md §5, rule 3). Releasing never restarts anything.
    pub fn release_estop(&self, source: estop::Source) -> Result<EstopView, MachineError> {
        let mut st = self.state.lock();
        let latched = st.running.iter().find_map(|(name, r)| match &r.runner {
            Some(Runner::Bridge(h)) if h.hardware_estop_latched() => Some(name.clone()),
            _ => None,
        });
        if let Some(bridge) = latched {
            return Err(err(
                ErrorKind::Conflict,
                "estop-latched",
                format!("The stop button on the board \"{bridge}\" is still pressed."),
                "Reset it on the board first, then release the e-stop here.",
            ));
        }
        if let Some(change) = st.estop.release(source, "Released", self.clock.now_us()) {
            tracing::warn!(target: "core::estop", "e-stop released");
            let _ = self.estop_events.send(change);
        }
        for b in st.running.values().filter_map(|r| r.runner.as_ref()) {
            if let Runner::Bridge(h) = b {
                h.set_estop(None);
            }
        }
        Ok(EstopView { engaged: None, history: st.estop.history().cloned().collect() })
    }

    /// Sets the value pattern of a manual simulated sensor.
    pub fn set_pattern(&self, part: &str, pattern: Pattern) -> Result<Pattern, MachineError> {
        let mut st = self.state.lock();
        let name = Self::known_part(&st, part)?;
        let runner = st.running.get_mut(&name).and_then(|r| r.runner.as_mut());
        let result = match runner {
            Some(Runner::Simulated(s)) => s.set_pattern(pattern),
            _ => Err(PatternError::NotManual),
        };
        result.map(|()| pattern).map_err(|e| pattern_error(&name, e))
    }

    /// The pattern a manual simulated sensor follows now.
    pub fn pattern(&self, part: &str) -> Result<Pattern, MachineError> {
        let st = self.state.lock();
        let name = Self::known_part(&st, part)?;
        match st.running.get(&name).and_then(|r| r.runner.as_ref()) {
            Some(Runner::Simulated(s)) => {
                s.pattern().ok_or_else(|| pattern_error(&name, PatternError::NotASensor))
            }
            _ => Err(pattern_error(&name, PatternError::NotManual)),
        }
    }

    /// Ends the leases that cover `part` (it is being removed or restarted).
    pub(super) fn revoke_part(&self, st: &mut State, part: &PartName, reason: EndReason) {
        let ended = st.leases.revoke_part(part, reason);
        self.finish(st, ended);
    }

    /// Ends the leases on `part` and, for a bridge, on each of its sub-parts, while their safe
    /// states can still reach the board (it is about to stop).
    pub(super) fn end_part(&self, st: &mut State, part: &PartName, reason: EndReason) {
        self.revoke_part(st, part, reason);
        let subs: Vec<PartName> = bridge_handle(st, part)
            .map(|h| h.sub_parts().into_iter().map(|s| s.name).collect())
            .unwrap_or_default();
        for sub in subs {
            self.revoke_part(st, &sub, reason);
        }
    }

    /// Acts on what bridges reported since the last sweep.
    fn handle_bridge_events(&self) {
        let events: Vec<BridgeEvent> =
            self.bridge_inbox.lock().expect("bridge inbox lock").try_iter().collect();
        for event in events {
            match event {
                BridgeEvent::HardwareEstop { bridge, reason } => {
                    tracing::warn!(target: "core::estop", part = %bridge, "the board's stop button was pressed");
                    self.engage_estop(estop::Source::Part, &reason);
                }
                BridgeEvent::PartsChanged { bridge, removed, changed } => {
                    tracing::info!(target: "core::bridge", part = %bridge, "the board has other parts now");
                    let mut st = self.state.lock();
                    for p in &removed {
                        self.revoke_part(&mut st, p, EndReason::PartRemoved);
                    }
                    for p in &changed {
                        self.revoke_part(&mut st, p, EndReason::PartRestarted);
                    }
                    drop(st);
                    self.status.touch();
                }
            }
        }
    }

    /// Ends every lease (the core is stopping).
    pub(super) fn revoke_all(&self, st: &mut State, reason: EndReason) {
        let ended = st.leases.revoke_all(reason);
        self.finish(st, ended);
    }

    /// Ends expired leases, and leases on parts that stopped working.
    pub(super) fn sweep(&self) {
        self.handle_bridge_events();
        let mut st = self.state.lock();
        let mut ended = st.leases.sweep(self.clock.now_us());
        let failed: Vec<PartName> = st
            .leases
            .active()
            .iter()
            .flat_map(|l| l.parts().cloned().collect::<Vec<_>>())
            .filter(|p| {
                matches!(
                    self.status.get(p),
                    Some(PartStatus::Failed(_) | PartStatus::Stopped(_) | PartStatus::Invalid(_))
                )
            })
            .collect();
        for p in failed {
            ended.extend(st.leases.revoke_part(&p, EndReason::PartFailed));
        }
        self.finish(&mut st, ended);
    }

    /// Every ended lease: its parts get their safe state, and everyone hears `lease-ended`.
    fn finish(&self, st: &mut State, ended: Vec<Ended>) {
        for e in ended {
            for part in e.lease.parts() {
                self.make_safe(st, part);
            }
            tracing::info!(
                target: "core::leases",
                "{}'s lease on {} ended: {}",
                e.lease.holder,
                e.lease.part,
                reason_words(e.reason)
            );
            let _ = self.lease_events.send(LeaseEvent::Ended(e));
        }
    }

    /// Sends the safe state of every leased capability `part` has. A sub-part's goes to its
    /// board as SAFE.
    fn make_safe(&self, st: &mut State, part: &PartName) {
        if part.is_sub() {
            let moves = provides_of(st, part).iter().any(|p| p.capability.lease().is_some());
            if let Some(b) = bridge_of(st, part).filter(|_| moves) {
                b.safe(part);
            }
            return;
        }
        let Some(Running { provides, runner: Some(runner), .. }) = st.running.get_mut(part) else { return };
        for policy in provides.iter().filter_map(|p| p.capability.lease()) {
            match runner.command(policy.safe_state) {
                Ok(_) => {}
                // A part that isn't running isn't moving either; it starts in its safe state.
                Err(CommandError::NotReady(_)) => {}
                Err(CommandError::Unsupported) => tracing::error!(
                    target: "core::leases",
                    "{part} refused its safe state ({}); its connector must accept it",
                    policy.safe_state.kind()
                ),
            }
        }
    }
}

fn lease_error(st: &State, part: &PartName, e: LeaseError) -> MachineError {
    match e {
        LeaseError::Ended(reason) => MachineError {
            lease_ended: Some(reason),
            plan_problems: Vec::new(),
            ..err(
                ErrorKind::Forbidden,
                "lease-ended",
                format!("Your lease on \"{part}\" has ended: {}.", reason_words(reason)),
                "Request a new lease to keep going (POST …/lease).",
            )
        },
        LeaseError::Required => {
            let message = match st.leases.covering(part) {
                Some(l) if &l.part != part => {
                    format!("\"{part}\" is part of \"{}\", which \"{}\" is driving.", l.part, l.holder)
                }
                Some(l) => format!("\"{}\" is driving \"{part}\", and that lease isn't yours.", l.holder),
                None => format!("Commanding \"{part}\" needs a lease, and this isn't one."),
            };
            err(
                ErrorKind::Forbidden,
                "lease-required",
                message,
                "Request a lease first (POST …/lease) and send its id.",
            )
        }
        LeaseError::Held { .. } => unreachable!("only request answers held"),
    }
}

fn refused(part: &PartName, r: Refused) -> MachineError {
    let message = match r {
        Refused::NotFinite { field } => format!("{field} must be a number."),
        Refused::Unsupported { kind } => format!("\"{part}\" doesn't take {kind} commands."),
        Refused::WrongCapability { command, part: has } => {
            format!("\"{part}\" is a {}; it doesn't take {} commands.", has.id(), command.id())
        }
    };
    err(
        ErrorKind::BadRequest,
        "bad-command",
        message,
        "Its info (GET /api/v0/connectors) says what it takes.",
    )
}

fn pattern_error(part: &PartName, e: PatternError) -> MachineError {
    match e {
        PatternError::NotManual => err(
            ErrorKind::Conflict,
            "not-manual",
            format!("\"{part}\" isn't a manual simulated part, so its values can't be set by hand."),
            "Set simulate = \"manual\" for the part in parts.toml (or add it again with Simulated ticked).",
        ),
        PatternError::NotASensor => err(
            ErrorKind::BadRequest,
            "not-a-sensor",
            format!("\"{part}\" has no sensor value to set; it reports the commands it gets."),
            "Drive it with a lease and commands instead.",
        ),
        PatternError::Invalid(m) => {
            err(ErrorKind::BadRequest, "bad-pattern", m, "Check the numbers in the pattern.")
        }
    }
}

/// Runs [`Machine::sweep`] until `stop` is set or the machine is gone.
/// Also reconciles the apps every [`APPS_EVERY`], so an app follows its parts (one removed,
/// a board's part appearing) without anyone asking.
pub(super) fn sweep_leases(machine: Weak<Machine>, stop: Arc<AtomicBool>) {
    let mut apps_checked = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(SWEEP_EVERY);
        match machine.upgrade() {
            Some(m) => {
                m.sweep();
                m.sweep_app_windows();
                if apps_checked.elapsed() >= APPS_EVERY {
                    m.reconcile_apps_if_free();
                    m.flush_app_states();
                    apps_checked = std::time::Instant::now();
                }
            }
            None => return,
        }
    }
}

/// How often the sweeper reconciles the apps with the parts.
const APPS_EVERY: Duration = Duration::from_millis(200);
