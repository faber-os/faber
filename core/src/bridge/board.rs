//! What a board says, in the core's terms: its parts as sub-parts, and its status as health in
//! words that say what to do.

use super::Link;
use crate::names::{PartName, board_part_name};
use faber_capabilities::{Info, planar_base};
use faber_connector::{Health, Problem, Provide};
use faberlink::message::{Cause, Fault, PartDescription, PartInfo};

/// Where the link to a board stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Saying hello; the board hasn't answered yet.
    Connecting,
    /// The board answered; collecting its parts.
    Describing,
    /// Parts are known: readings flow, commands may be sent.
    Ready,
    /// The board hasn't answered a hello for a while.
    NoAnswer,
    /// The port closed, or a connected board went quiet. Faber keeps trying.
    Lost,
    /// The board speaks another FaberLink version.
    Incompatible,
}

impl LinkState {
    pub fn id(self) -> &'static str {
        match self {
            LinkState::Connecting => "connecting",
            LinkState::Describing => "describing",
            LinkState::Ready => "ready",
            LinkState::NoAnswer => "no-answer",
            LinkState::Lost => "lost",
            LinkState::Incompatible => "incompatible",
        }
    }
}

/// One part on the board, as a part of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct SubPart {
    /// `mega.left`.
    pub name: PartName,
    /// The board's own name for it, as the sketch wrote it.
    pub board_name: String,
    /// The board's id for it, in every message about it.
    pub id: u8,
    /// The capability id the board gave, e.g. `motor@1`, even one this core doesn't know.
    pub capability: String,
    /// Its one signal, named after its capability (`mega.left/motor`), with its info. `None`
    /// when this core doesn't know the capability.
    pub provide: Option<Provide>,
    /// For actuators: the board stops it by itself after this long without a command.
    pub lease_ms: u16,
}

/// The board's parts as sub-parts of `bridge`. A name the board gives that can't be a part name
/// (or that two parts share) becomes `part<id>`. A drive base's built-from list names the board's
/// parts; it is turned into their sub-part names, so a lease on the base locks them.
pub fn sub_parts(bridge: &PartName, parts: &[PartDescription]) -> Vec<SubPart> {
    let mut used: Vec<String> = Vec::new();
    let local: Vec<String> = parts
        .iter()
        .map(|p| {
            let name = board_part_name(&p.name)
                .filter(|n| !used.contains(n))
                .unwrap_or_else(|| format!("part{}", p.id));
            used.push(name.clone());
            name
        })
        .collect();
    let full = |board_name: &str| -> Option<String> {
        let i = parts.iter().position(|p| p.name == board_name)?;
        PartName::sub(bridge, &local[i]).ok().map(|n| n.to_string())
    };
    parts
        .iter()
        .zip(&local)
        .filter_map(|(p, name)| {
            let name = PartName::sub(bridge, name).ok()?;
            let provide = match (&p.info, p.known_capability()) {
                (PartInfo::Known(info), Some(capability)) if info.capability() == capability => {
                    let info = match info.clone() {
                        Info::PlanarBase(i) => Info::PlanarBase(planar_base::Info {
                            built_from: i
                                .built_from
                                .iter()
                                .map(|b| full(b).unwrap_or_else(|| b.clone()))
                                .collect(),
                            ..i
                        }),
                        other => other,
                    };
                    Some(Provide::with_info(info))
                }
                _ => None,
            };
            Some(SubPart {
                name,
                board_name: p.name.clone(),
                id: p.id,
                capability: p.capability.clone(),
                provide,
                lease_ms: p.lease_ms,
            })
        })
        .collect()
}

/// A connected sub-part runs, unless this core doesn't know what it is.
pub fn sub_health(s: &SubPart) -> Health {
    match &s.provide {
        Some(_) => Health::Running,
        None => Health::Degraded(Problem::new(
            format!(
                "The board says \"{}\" is a {}, which this Faber doesn't know.",
                s.board_name, s.capability
            ),
            "Update Faber, or check the part in the board's sketch.",
        )),
    }
}

/// A connected board's health from its last STATUS. `silent`: no STATUS lately. `recent_bad`:
/// the board's count of dropped frames, if it grew in the last few seconds.
pub(super) fn bridge_health(l: &Link, silent: bool, recent_bad: Option<u16>) -> Health {
    if silent {
        return Health::Degraded(Problem::new(
            "The board stopped sending its status.",
            "Its firmware may be old, or the sketch may keep loop() busy: link.poll() must run often.",
        ));
    }
    if let Some(f) = l.fault {
        return Health::Failed(Problem::new(
            format!("The board reports a fault: {}.", fault_words(f)),
            "Check its wiring and power, then reset the board. Its log in Logs may say more.",
        ));
    }
    if l.hardware_estop {
        return Health::Degraded(Problem::new(
            "The stop button on the board is pressed.",
            "Reset it on the board, then release the e-stop in Faber.",
        ));
    }
    if l.failsafe {
        return Health::Degraded(Problem::new(
            "The board stopped its motors: Faber's heartbeat didn't reach it in time.",
            "Check the cable. A very busy or sleeping computer can cause it too. Motors start again on the next command.",
        ));
    }
    if let Some(n) = recent_bad {
        return Health::Degraded(Problem::new(
            format!("The board has thrown away {n} damaged messages."),
            "Check the cable: short and shielded is best, away from motor wires.",
        ));
    }
    Health::Running
}

/// The last thing that went wrong on the board, in words.
fn fault_words(f: Fault) -> &'static str {
    match f {
        Fault::None => "it doesn't say which",
        Fault::HeartbeatLost => "Faber's heartbeat stopped reaching it",
        Fault::LeaseExpired => "a part's lease ran out",
        Fault::BadFrames => "damaged messages",
        Fault::RefusedCommand => "it refused a command",
        Fault::Estop => "the e-stop",
    }
}

/// Why the board stopped an actuator by itself, for the log.
pub fn cause_words(c: Cause) -> &'static str {
    match c {
        Cause::Command => "a command",
        Cause::Refused => "the board refused a command",
        Cause::HeartbeatLost => "Faber's heartbeat stopped reaching the board",
        Cause::LeaseExpired => "no command arrived within its lease time",
        Cause::Safe => "Faber asked for its safe state",
        Cause::Estop => "the e-stop is engaged",
        Cause::NoHeartbeat => "a command came without a live heartbeat",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{Capability, Twist2d, distance, motor};

    fn part(id: u8, name: &str, info: Info) -> PartDescription {
        PartDescription {
            id,
            count: 3,
            lease_ms: if info.capability() == Capability::Distance { 0 } else { 250 },
            capability: info.capability().id().into(),
            name: name.into(),
            info: PartInfo::Known(info),
        }
    }

    fn motor_info() -> Info {
        Info::Motor(motor::Info {
            kind: motor::Kind::Dc,
            modes: motor::Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: Some(10.0),
        })
    }

    fn mega() -> PartName {
        PartName::parse("mega").unwrap()
    }

    #[test]
    fn the_boards_parts_become_sub_parts_with_one_signal_each() {
        let d = Info::Distance(distance::Info {
            min_m: 0.1,
            max_m: 0.8,
            field_of_view_rad: 0.1,
            kind: distance::Kind::Infrared,
        });
        let subs = sub_parts(&mega(), &[part(0, "left", motor_info()), part(1, "Front IR", d)]);
        let names: Vec<_> = subs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["mega.left", "mega.front_ir"]);
        assert_eq!(subs[1].board_name, "Front IR");
        let p = subs[0].provide.as_ref().unwrap();
        assert_eq!((p.name.as_str(), p.capability), ("motor", Capability::Motor));
        assert_eq!(p.info, Some(motor_info()));
        assert_eq!(sub_health(&subs[0]), Health::Running);
    }

    #[test]
    fn unusable_or_repeated_names_become_part_ids() {
        let subs = sub_parts(
            &mega(),
            &[part(0, "left", motor_info()), part(1, "left", motor_info()), part(2, "#", motor_info())],
        );
        let names: Vec<_> = subs.iter().map(|s| s.name.local().to_string()).collect();
        assert_eq!(names, ["left", "part1", "part2"]);
    }

    #[test]
    fn a_capability_this_core_doesnt_know_is_shown_not_dropped() {
        let unknown = PartDescription {
            id: 0,
            count: 1,
            lease_ms: 0,
            capability: "thermal-camera@1".into(),
            name: "eye".into(),
            info: PartInfo::Unknown(vec![1, 2]),
        };
        let subs = sub_parts(&mega(), &[unknown]);
        assert_eq!(subs[0].provide, None);
        let Health::Degraded(p) = sub_health(&subs[0]) else { panic!() };
        assert!(p.message.contains("thermal-camera@1"), "{p:?}");
    }

    #[test]
    fn a_base_is_built_from_the_sub_parts_its_board_names() {
        let base = Info::PlanarBase(planar_base::Info {
            holonomic: false,
            max_velocity: Twist2d { vx: 0.5, vy: 0.0, omega: 1.0 },
            max_acceleration: Twist2d { vx: 1.0, vy: 0.0, omega: 2.0 },
            built_from: vec!["Left".into(), "right".into(), "nothing".into()],
        });
        let subs = sub_parts(
            &mega(),
            &[part(0, "Left", motor_info()), part(1, "right", motor_info()), part(2, "base", base)],
        );
        let info = subs[2].provide.as_ref().unwrap().info.as_ref().unwrap();
        assert_eq!(info.built_from(), ["mega.left", "mega.right", "nothing"]);
    }
}
