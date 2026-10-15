//! The machine's safety wiring (D12, docs/simulation.md, sections 5 and 6): leases, commands
//! and the e-stop. The lease table (`leases`) and the e-stop state (`estop`) are pure; this is
//! where they meet the running parts:
//! - every command is checked against the e-stop and the lease, clamped to the part's info, and
//!   renews the lease;
//! - whenever a lease ends, for any reason, its parts get their safe state and `lease-ended` is
//!   announced;
//! - a sweeper ends expired leases (and leases on parts that failed) within 50 ms.

use super::{ErrorKind, Machine, MachineError, Running, State};
use crate::leases::{EndReason, Ended, Lease, LeaseError, Request};
use crate::names::PartName;
use crate::runtime::PartStatus;
use crate::simulation::{Pattern, PatternError, SimulatedPart};
use faber_capabilities::{Command, Info, Refused, estop};
use faber_connector::{Applied, CommandError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// How often the sweeper looks for expired leases. Well inside the 50 ms the design promises.
pub(super) const SWEEP_EVERY: Duration = Duration::from_millis(10);

/// What runs a part: the runtime (a real part on its port) or a simulation.
pub(super) enum Runner {
    Real(crate::runtime::PartHandle),
    Simulated(Box<dyn SimulatedPart>),
}

impl Runner {
    fn command(&mut self, command: Command) -> Result<Applied, CommandError> {
        match self {
            Runner::Real(h) => h.command(command),
            Runner::Simulated(s) => s.command(command),
        }
    }

    pub(super) fn stop(self) {
        match self {
            Runner::Real(h) => h.stop(),
            Runner::Simulated(s) => drop(s),
        }
    }
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
        EndReason::Shutdown => "Faber stopped",
    }
}

impl Machine {
    /// Fires on every lease granted or ended.
    pub fn lease_events(&self) -> tokio::sync::broadcast::Receiver<LeaseEvent> {
        self.lease_events.subscribe()
    }

    /// Fires on every engage and release of the e-stop. None is lost (every-message).
    pub fn estop_events(&self) -> tokio::sync::broadcast::Receiver<estop::Reading> {
        self.estop_events.subscribe()
    }

    /// Every live lease, by part.
    pub fn leases(&self) -> Vec<Lease> {
        self.state.lock().expect("machine lock").leases.active()
    }

    pub fn estop(&self) -> EstopView {
        let st = self.state.lock().expect("machine lock");
        EstopView { engaged: st.estop.engaged().cloned(), history: st.estop.history().cloned().collect() }
    }

    fn known_part(st: &State, name: &str) -> Result<PartName, MachineError> {
        PartName::parse(name)
            .ok()
            .filter(|n| st.config.parts.contains_key(n))
            .ok_or_else(|| unknown_part(name))
    }

    /// Grants `holder` the lease on `part`, or says who holds it. `take_over` ends the lease in
    /// the way (an operator taking over; its holder hears `taken-over`).
    pub fn request_lease(&self, part: &str, holder: &str, take_over: bool) -> Result<Lease, MachineError> {
        let mut st = self.state.lock().expect("machine lock");
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
        let running = st.running.get(&name);
        let leased = running
            .and_then(|r| r.provides.iter().find_map(|p| Some((p.capability.lease()?, p.info.clone()))));
        let Some((policy, info)) = leased else {
            return Err(err(
                ErrorKind::BadRequest,
                "not-leasable",
                format!("\"{name}\" has nothing that moves, so it takes no lease."),
                "Leases are for actuators such as motors and drive bases.",
            ));
        };
        let locks = info.as_ref().map(Info::built_from).unwrap_or_default();
        let locks = locks.iter().filter_map(|n| PartName::parse(n).ok()).collect();
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
        let mut st = self.state.lock().expect("machine lock");
        let name = Self::known_part(&st, part)?;
        let now = self.clock.now_us();
        st.leases.renew(&name, lease, now).map_err(|e| lease_error(&st, &name, e))
    }

    /// The holder is done: the part gets its safe state.
    pub fn release_lease(&self, part: &str, lease: &str) -> Result<(), MachineError> {
        let mut st = self.state.lock().expect("machine lock");
        let name = Self::known_part(&st, part)?;
        let ended = st.leases.release(&name, lease).map_err(|e| lease_error(&st, &name, e))?;
        self.finish(&mut st, vec![ended]);
        Ok(())
    }

    /// Checks the e-stop and the lease, clamps the command to the part's info, applies it and
    /// renews the lease.
    pub fn command(&self, part: &str, lease: &str, command: Command) -> Result<CommandReply, MachineError> {
        let mut st = self.state.lock().expect("machine lock");
        let name = Self::known_part(&st, part)?;
        if st.estop.is_engaged() {
            return Err(estop_engaged(&st));
        }
        let provide = st
            .running
            .get(&name)
            .and_then(|r| r.provides.iter().find(|p| p.capability == command.capability()))
            .cloned();
        let Some(provide) = provide else {
            let has: Vec<_> = st
                .running
                .get(&name)
                .map(|r| r.provides.iter().map(|p| p.capability.id()).collect())
                .unwrap_or_default();
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
            None => faber_capabilities::Clamped { command, clamped: false },
        };
        let runner = st.running.get_mut(&name).and_then(|r| r.runner.as_mut());
        let applied = match runner {
            Some(r) => r.command(clamped.command),
            None => Err(CommandError::NotReady(faber_connector::Problem::new(
                format!("\"{name}\" isn't running."),
                "Check its state in Parts.",
            ))),
        };
        let applied = applied.map_err(|e| match e {
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
        let mut st = self.state.lock().expect("machine lock");
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
        EstopView { engaged: st.estop.engaged().cloned(), history: st.estop.history().cloned().collect() }
    }

    pub fn release_estop(&self, source: estop::Source) -> EstopView {
        let mut st = self.state.lock().expect("machine lock");
        if let Some(change) = st.estop.release(source, "Released", self.clock.now_us()) {
            tracing::warn!(target: "core::estop", "e-stop released");
            let _ = self.estop_events.send(change);
        }
        EstopView { engaged: None, history: st.estop.history().cloned().collect() }
    }

    /// Sets the value pattern of a manual simulated sensor.
    pub fn set_pattern(&self, part: &str, pattern: Pattern) -> Result<Pattern, MachineError> {
        let mut st = self.state.lock().expect("machine lock");
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
        let st = self.state.lock().expect("machine lock");
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

    /// Ends every lease (the core is stopping).
    pub(super) fn revoke_all(&self, st: &mut State, reason: EndReason) {
        let ended = st.leases.revoke_all(reason);
        self.finish(st, ended);
    }

    /// Ends expired leases, and leases on parts that stopped working.
    pub(super) fn sweep(&self) {
        let mut st = self.state.lock().expect("machine lock");
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

    /// Sends the safe state of every leased capability `part` has.
    fn make_safe(&self, st: &mut State, part: &PartName) {
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
        LeaseError::Ended(reason) => err(
            ErrorKind::Forbidden,
            "lease-ended",
            format!("Your lease on \"{part}\" has ended: {}.", reason_words(reason)),
            "Request a new lease to keep going (POST …/lease).",
        ),
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
pub(super) fn sweep_leases(machine: Weak<Machine>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(SWEEP_EVERY);
        match machine.upgrade() {
            Some(m) => m.sweep(),
            None => return,
        }
    }
}
