//! Faber capability data types: the Rust mirror of `wit/capabilities/`.
//!
//! Only capabilities that are in use are implemented here; the rest of the starter set is
//! specified in `docs/capabilities.md` and added when its first connector (or simulation) arrives.
//!
//! Wire conventions (docs/api.md): field names are camelCase, every reading carries `tUs`, and
//! a [`Reading`], [`Command`] or [`Info`] is tagged with its capability id, e.g. `"laser-scan@1"`.

pub mod analog_in;
pub mod battery;
pub mod digital_in;
pub mod digital_out;
pub mod distance;
pub mod estop;
mod geometry;
pub mod laser_scan;
pub mod motor;
mod mount;
mod non_finite;
pub mod planar_base;
pub mod pose;
pub mod position;
pub mod pwm_out;
pub mod temperature;

pub use geometry::{Quaternion, Twist2d, Vec3};
pub use mount::Mount;

use serde::{Deserialize, Serialize};

/// Microseconds on the core's monotonic clock.
pub type Timestamp = u64;

/// How the router delivers a capability's signal (docs/capabilities.md, rule 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// A slow subscriber gets the newest reading and skips the rest.
    LatestValue,
    /// Events: no reading may be lost.
    EveryMessage,
}

/// How a leased capability is protected (docs/capabilities.md, "The lease").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeasePolicy {
    /// How long a lease lasts without a command or a renewal.
    pub duration_us: u64,
    /// What the core sends the part whenever a lease ends, and on the e-stop.
    pub safe_state: Command,
}

/// Every capability the core knows, with its versioned id.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "laser-scan@1")]
    LaserScan,
    #[serde(rename = "distance@1")]
    Distance,
    #[serde(rename = "motor@1")]
    Motor,
    #[serde(rename = "planar-base@1")]
    PlanarBase,
    #[serde(rename = "estop@1")]
    Estop,
    #[serde(rename = "pose@1")]
    Pose,
    #[serde(rename = "position@1")]
    Position,
    #[serde(rename = "battery@1")]
    Battery,
    #[serde(rename = "temperature@1")]
    Temperature,
    #[serde(rename = "digital-in@1")]
    DigitalIn,
    #[serde(rename = "digital-out@1")]
    DigitalOut,
    #[serde(rename = "analog-in@1")]
    AnalogIn,
    #[serde(rename = "pwm-out@1")]
    PwmOut,
}

impl Capability {
    pub const ALL: &'static [Capability] = &[
        Capability::LaserScan,
        Capability::Distance,
        Capability::Motor,
        Capability::PlanarBase,
        Capability::Estop,
        Capability::Pose,
        Capability::Position,
        Capability::Battery,
        Capability::Temperature,
        Capability::DigitalIn,
        Capability::DigitalOut,
        Capability::AnalogIn,
        Capability::PwmOut,
    ];

    /// The versioned id used in manifests, addresses and on the wire.
    pub fn id(self) -> &'static str {
        match self {
            Capability::LaserScan => "laser-scan@1",
            Capability::Distance => "distance@1",
            Capability::Motor => "motor@1",
            Capability::PlanarBase => "planar-base@1",
            Capability::Estop => "estop@1",
            Capability::Pose => "pose@1",
            Capability::Position => "position@1",
            Capability::Battery => "battery@1",
            Capability::Temperature => "temperature@1",
            Capability::DigitalIn => "digital-in@1",
            Capability::DigitalOut => "digital-out@1",
            Capability::AnalogIn => "analog-in@1",
            Capability::PwmOut => "pwm-out@1",
        }
    }

    /// The id without its version, used as the default signal name.
    pub fn name(self) -> &'static str {
        let id = self.id();
        &id[..id.find('@').expect("ids are versioned")]
    }

    pub fn delivery(self) -> Delivery {
        match self {
            Capability::Estop | Capability::DigitalIn => Delivery::EveryMessage,
            Capability::LaserScan
            | Capability::Distance
            | Capability::Motor
            | Capability::PlanarBase
            | Capability::Pose
            | Capability::Position
            | Capability::Battery
            | Capability::Temperature
            | Capability::DigitalOut
            | Capability::AnalogIn
            | Capability::PwmOut => Delivery::LatestValue,
        }
    }

    /// `Some` for capabilities whose commands need a lease (🔒), with its duration and safe state.
    pub fn lease(self) -> Option<LeasePolicy> {
        match self {
            Capability::Motor => {
                Some(LeasePolicy { duration_us: 250_000, safe_state: Command::Motor(motor::Command::STOP) })
            }
            Capability::PlanarBase => Some(LeasePolicy {
                duration_us: 250_000,
                safe_state: Command::PlanarBase(planar_base::Command::STOP),
            }),
            // An output can switch something that moves: 1 s, then its safe level (inactive).
            Capability::DigitalOut => Some(LeasePolicy {
                duration_us: 1_000_000,
                safe_state: Command::DigitalOut(digital_out::Command::SAFE),
            }),
            Capability::PwmOut => Some(LeasePolicy {
                duration_us: 1_000_000,
                safe_state: Command::PwmOut(pwm_out::Command::OFF),
            }),
            Capability::LaserScan
            | Capability::Distance
            | Capability::Estop
            | Capability::Pose
            | Capability::Position
            | Capability::Battery
            | Capability::Temperature
            | Capability::DigitalIn
            | Capability::AnalogIn => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Capability> {
        Capability::ALL.iter().copied().find(|c| c.id() == id)
    }
}

/// One reading of any capability, tagged with the capability it belongs to.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "capability", content = "reading")]
pub enum Reading {
    #[serde(rename = "laser-scan@1")]
    LaserScan(laser_scan::Reading),
    #[serde(rename = "distance@1")]
    Distance(distance::Reading),
    #[serde(rename = "motor@1")]
    Motor(motor::Reading),
    #[serde(rename = "planar-base@1")]
    PlanarBase(planar_base::Reading),
    #[serde(rename = "estop@1")]
    Estop(estop::Reading),
    #[serde(rename = "pose@1")]
    Pose(pose::Reading),
    #[serde(rename = "position@1")]
    Position(position::Reading),
    #[serde(rename = "battery@1")]
    Battery(battery::Reading),
    #[serde(rename = "temperature@1")]
    Temperature(temperature::Reading),
    #[serde(rename = "digital-in@1")]
    DigitalIn(digital_in::Reading),
    #[serde(rename = "digital-out@1")]
    DigitalOut(digital_out::Reading),
    #[serde(rename = "analog-in@1")]
    AnalogIn(analog_in::Reading),
    #[serde(rename = "pwm-out@1")]
    PwmOut(pwm_out::Reading),
}

impl Reading {
    pub fn capability(&self) -> Capability {
        match self {
            Reading::LaserScan(_) => Capability::LaserScan,
            Reading::Distance(_) => Capability::Distance,
            Reading::Motor(_) => Capability::Motor,
            Reading::PlanarBase(_) => Capability::PlanarBase,
            Reading::Estop(_) => Capability::Estop,
            Reading::Pose(_) => Capability::Pose,
            Reading::Position(_) => Capability::Position,
            Reading::Battery(_) => Capability::Battery,
            Reading::Temperature(_) => Capability::Temperature,
            Reading::DigitalIn(_) => Capability::DigitalIn,
            Reading::DigitalOut(_) => Capability::DigitalOut,
            Reading::AnalogIn(_) => Capability::AnalogIn,
            Reading::PwmOut(_) => Capability::PwmOut,
        }
    }

    pub fn t_us(&self) -> Timestamp {
        match self {
            Reading::LaserScan(r) => r.t_us,
            Reading::Distance(r) => r.t_us,
            Reading::Motor(r) => r.t_us,
            Reading::PlanarBase(r) => r.t_us,
            Reading::Estop(r) => r.t_us,
            Reading::Pose(r) => r.t_us,
            Reading::Position(r) => r.t_us,
            Reading::Battery(r) => r.t_us,
            Reading::Temperature(r) => r.t_us,
            Reading::DigitalIn(r) => r.t_us,
            Reading::DigitalOut(r) => r.t_us,
            Reading::AnalogIn(r) => r.t_us,
            Reading::PwmOut(r) => r.t_us,
        }
    }
}

/// One command to an actuator, tagged with its capability like a [`Reading`]:
/// `{"capability": "planar-base@1", "command": {"kind": "set-velocity", "vx": 0.2, "vy": 0, "omega": 0}}`.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "capability", content = "command")]
pub enum Command {
    #[serde(rename = "motor@1")]
    Motor(motor::Command),
    #[serde(rename = "planar-base@1")]
    PlanarBase(planar_base::Command),
    #[serde(rename = "digital-out@1")]
    DigitalOut(digital_out::Command),
    #[serde(rename = "pwm-out@1")]
    PwmOut(pwm_out::Command),
}

impl Command {
    pub fn capability(&self) -> Capability {
        match self {
            Command::Motor(_) => Capability::Motor,
            Command::PlanarBase(_) => Capability::PlanarBase,
            Command::DigitalOut(_) => Capability::DigitalOut,
            Command::PwmOut(_) => Capability::PwmOut,
        }
    }

    /// The command's own kind, e.g. `set-velocity`.
    pub fn kind(&self) -> &'static str {
        match self {
            Command::Motor(c) => c.kind(),
            Command::PlanarBase(c) => c.kind(),
            Command::DigitalOut(c) => c.kind(),
            Command::PwmOut(c) => c.kind(),
        }
    }

    /// Checks the command against the part's info and clamps it to its limits (an overshooting
    /// joystick is clamped, not refused). `info` must be for the command's capability.
    pub fn clamp(self, info: &Info) -> Result<Clamped<Command>, Refused> {
        match (self, info) {
            (Command::Motor(c), Info::Motor(i)) => Ok(c.clamp(i)?.map(Command::Motor)),
            (Command::PlanarBase(c), Info::PlanarBase(i)) => Ok(c.clamp(i)?.map(Command::PlanarBase)),
            _ => Err(Refused::WrongCapability { command: self.capability(), part: info.capability() }),
        }
    }

    /// Clamps a command for a part that has no info to the limits every part of its capability
    /// has (a PWM duty is always 0–1). Others pass unchanged.
    pub fn clamp_bare(self) -> Result<Clamped<Command>, Refused> {
        match self {
            Command::PwmOut(c) => Ok(c.clamp()?.map(Command::PwmOut)),
            other => Ok(Clamped { command: other, clamped: false }),
        }
    }
}

/// What a part reports about itself once (docs/capabilities.md, rule 4), tagged like a [`Reading`].
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "capability", content = "info")]
pub enum Info {
    #[serde(rename = "laser-scan@1")]
    LaserScan(laser_scan::Info),
    #[serde(rename = "distance@1")]
    Distance(distance::Info),
    #[serde(rename = "motor@1")]
    Motor(motor::Info),
    #[serde(rename = "planar-base@1")]
    PlanarBase(planar_base::Info),
    #[serde(rename = "estop@1")]
    Estop(estop::Info),
    #[serde(rename = "battery@1")]
    Battery(battery::Info),
}

impl Info {
    pub fn capability(&self) -> Capability {
        match self {
            Info::LaserScan(_) => Capability::LaserScan,
            Info::Distance(_) => Capability::Distance,
            Info::Motor(_) => Capability::Motor,
            Info::PlanarBase(_) => Capability::PlanarBase,
            Info::Estop(_) => Capability::Estop,
            Info::Battery(_) => Capability::Battery,
        }
    }

    /// The parts this one is built from (a lease on it locks them too).
    pub fn built_from(&self) -> &[String] {
        match self {
            Info::PlanarBase(i) => &i.built_from,
            _ => &[],
        }
    }
}

/// A command after [`Command::clamp`], and whether clamping changed it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clamped<T> {
    pub command: T,
    pub clamped: bool,
}

impl<T> Clamped<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Clamped<U> {
        Clamped { command: f(self.command), clamped: self.clamped }
    }
}

impl Clamped<f32> {
    /// Clamps one value to `lo..=hi`; a value that is not a number is refused.
    pub fn of(value: f32, field: &'static str, lo: f32, hi: f32) -> Result<Clamped<f32>, Refused> {
        if !value.is_finite() {
            return Err(Refused::NotFinite { field });
        }
        let v = value.clamp(lo, hi);
        Ok(Clamped { command: v, clamped: v != value })
    }
}

/// Why a command can't be used at all (as opposed to being clamped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// A field is NaN or infinite.
    NotFinite { field: &'static str },
    /// The part's info says it doesn't accept this kind of command.
    Unsupported { kind: &'static str },
    /// The command is for another capability than the part's.
    WrongCapability { command: Capability, part: Capability },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ids_round_trip() {
        for &c in Capability::ALL {
            assert_eq!(Capability::from_id(c.id()), Some(c));
            assert!(c.id().starts_with(c.name()));
            assert_eq!(serde_json::to_value(c).unwrap(), c.id());
        }
        assert_eq!(Capability::from_id("laser-scan@2"), None);
        assert_eq!(Capability::PlanarBase.name(), "planar-base");
    }

    #[test]
    fn capability_serialises_as_its_id() {
        let json = serde_json::to_string(&Capability::LaserScan).unwrap();
        assert_eq!(json, "\"laser-scan@1\"");
    }

    #[test]
    fn each_capability_states_delivery_and_lease() {
        assert_eq!(Capability::Estop.delivery(), Delivery::EveryMessage);
        assert_eq!(Capability::Distance.delivery(), Delivery::LatestValue);
        let base = Capability::PlanarBase.lease().unwrap();
        assert_eq!(base.duration_us, 250_000);
        assert_eq!(base.safe_state, Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::ZERO)));
        let motor = Capability::Motor.lease().unwrap();
        assert_eq!(motor.safe_state, Command::Motor(motor::Command::SetVelocity { velocity_rad_s: 0.0 }));
        for c in [
            Capability::LaserScan,
            Capability::Distance,
            Capability::Estop,
            Capability::Pose,
            Capability::Position,
            Capability::Battery,
            Capability::Temperature,
            Capability::DigitalIn,
            Capability::AnalogIn,
        ] {
            assert!(c.lease().is_none(), "{c:?} is not leased");
        }
        assert_eq!(Capability::DigitalIn.delivery(), Delivery::EveryMessage, "every edge counts");
        let out = Capability::DigitalOut.lease().unwrap();
        assert_eq!(out.duration_us, 1_000_000);
        assert_eq!(out.safe_state, Command::DigitalOut(digital_out::Command::Set { level: false }));
        let pwm = Capability::PwmOut.lease().unwrap();
        assert_eq!(pwm.safe_state, Command::PwmOut(pwm_out::Command::SetDuty { duty: 0.0 }));
        for &c in Capability::ALL {
            if let Some(p) = c.lease() {
                assert_eq!(
                    p.safe_state.capability(),
                    c,
                    "the safe state is a command for the capability itself"
                );
            }
        }
    }

    #[test]
    fn reading_is_tagged_with_its_capability() {
        let r = Reading::LaserScan(laser_scan::Reading::even_bins(5, 0.0, 0.1, 0.1, vec![1.0]));
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["capability"], "laser-scan@1");
        assert_eq!(v["reading"]["tUs"], 5);
        assert_eq!(r.capability(), Capability::LaserScan);
        assert_eq!(r.t_us(), 5);
    }

    fn one_of_each() -> Vec<Reading> {
        vec![
            Reading::LaserScan(laser_scan::Reading::even_bins(9, 0.0, 0.1, 0.1, vec![f32::INFINITY])),
            Reading::Distance(distance::Reading { t_us: 1, meters: 0.5, status: distance::Status::Ok }),
            Reading::Motor(motor::Reading { t_us: 2, velocity_rad_s: Some(1.0), ..Default::default() }),
            Reading::PlanarBase(planar_base::Reading {
                t_us: 3,
                commanded: Twist2d::new(0.1, 0.0, 0.0),
                measured: Some(Twist2d::ZERO),
                bumped: true,
            }),
            Reading::Estop(estop::Reading {
                t_us: 4,
                engaged: true,
                source: estop::Source::App,
                reason: "x".into(),
            }),
            Reading::Pose(pose::Reading::planar(5, "world", 1.0, 1.0, 0.0)),
            Reading::Position(position::Reading::no_fix(6, 3)),
            Reading::Battery(battery::Reading::volts(7, 11.1, battery::Level::Ok)),
            Reading::Temperature(temperature::Reading::celsius(8, 21.5)),
            Reading::DigitalIn(digital_in::Reading { t_us: 9, level: true }),
            Reading::DigitalOut(digital_out::Reading { t_us: 10, level: false }),
            Reading::AnalogIn(analog_in::Reading { t_us: 11, fraction: 0.25, volts: Some(0.825) }),
            Reading::PwmOut(pwm_out::Reading { t_us: 12, duty: 0.5 }),
        ]
    }

    #[test]
    fn every_reading_round_trips_and_knows_its_capability() {
        let readings = one_of_each();
        let caps: Vec<_> = readings.iter().map(Reading::capability).collect();
        assert_eq!(caps, Capability::ALL, "one reading per capability");
        for r in readings {
            let v = serde_json::to_value(&r).unwrap();
            assert_eq!(v["capability"], r.capability().id());
            assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
            let bytes = rmp_serde::to_vec_named(&r).unwrap();
            assert_eq!(rmp_serde::from_slice::<Reading>(&bytes).unwrap(), r);
        }
    }

    #[test]
    fn commands_are_tagged_like_readings() {
        let c: Command = serde_json::from_value(serde_json::json!({
            "capability": "planar-base@1",
            "command": {"kind": "set-velocity", "vx": 0.2, "vy": 0, "omega": 0}
        }))
        .unwrap();
        assert_eq!(c, Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.2, 0.0, 0.0))));
        assert_eq!(c.kind(), "set-velocity");
        let m = Command::Motor(motor::Command::SetDuty { duty: 0.5 });
        let v = serde_json::to_value(m).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"capability": "motor@1", "command": {"kind": "set-duty", "duty": 0.5}})
        );
        assert_eq!(serde_json::from_value::<Command>(v).unwrap(), m);
        assert_eq!(rmp_serde::from_slice::<Command>(&rmp_serde::to_vec_named(&m).unwrap()).unwrap(), m);
        let bad =
            serde_json::from_value::<Command>(serde_json::json!({"capability": "distance@1", "command": {}}));
        assert!(bad.is_err(), "distance takes no commands");
    }

    #[test]
    fn a_command_for_another_capability_is_refused_by_clamp() {
        let info = Info::Distance(distance::Info {
            min_m: 0.0,
            max_m: 1.0,
            field_of_view_rad: 0.1,
            kind: distance::Kind::Infrared,
        });
        assert_eq!(
            Command::Motor(motor::Command::STOP).clamp(&info),
            Err(Refused::WrongCapability { command: Capability::Motor, part: Capability::Distance })
        );
    }

    #[test]
    fn info_is_tagged_and_round_trips() {
        let i = Info::PlanarBase(planar_base::Info {
            holonomic: true,
            max_velocity: Twist2d::new(1.0, 1.0, 1.0),
            max_acceleration: Twist2d::new(1.0, 1.0, 1.0),
            built_from: vec!["fl".into()],
        });
        let v = serde_json::to_value(&i).unwrap();
        assert_eq!(v["capability"], "planar-base@1");
        assert_eq!(v["info"]["builtFrom"][0], "fl");
        assert_eq!(serde_json::from_value::<Info>(v).unwrap(), i);
        assert_eq!(i.built_from(), ["fl".to_string()]);
        let l = Info::LaserScan(laser_scan::Info { range_min_m: 0.02, range_max_m: 12.0 });
        assert_eq!(rmp_serde::from_slice::<Info>(&rmp_serde::to_vec_named(&l).unwrap()).unwrap(), l);
        assert!(l.built_from().is_empty());
    }

    #[test]
    fn gpio_commands_are_tagged_and_a_bare_duty_is_clamped() {
        let c: Command = serde_json::from_value(serde_json::json!({
            "capability": "digital-out@1", "command": {"kind": "set", "level": true}
        }))
        .unwrap();
        assert_eq!(c, Command::DigitalOut(digital_out::Command::Set { level: true }));
        assert_eq!(c.kind(), "set");
        assert_eq!(c.clamp_bare(), Ok(Clamped { command: c, clamped: false }));
        let duty = Command::PwmOut(pwm_out::Command::SetDuty { duty: 2.0 });
        assert_eq!(
            duty.clamp_bare(),
            Ok(Clamped { command: Command::PwmOut(pwm_out::Command::SetDuty { duty: 1.0 }), clamped: true })
        );
        assert_eq!(duty.capability(), Capability::PwmOut);
    }

    #[test]
    fn clamped_values_say_whether_they_changed() {
        assert_eq!(Clamped::of(2.0, "x", -1.0, 1.0), Ok(Clamped { command: 1.0, clamped: true }));
        assert_eq!(Clamped::of(0.5, "x", -1.0, 1.0), Ok(Clamped { command: 0.5, clamped: false }));
        assert_eq!(Clamped::of(f32::NAN, "x", -1.0, 1.0), Err(Refused::NotFinite { field: "x" }));
    }
}
