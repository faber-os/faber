//! Fixed binary layouts for each capability's reading, command and info (docs/faberlink.md §4).
//!
//! They map 1:1 onto the types in `faber-capabilities`. Every number is little-endian, every
//! float an IEEE-754 `f32`. A capability that has no layout yet can't travel over FaberLink.

use crate::bytes::{Reader, Writer};
use crate::{DecodeError, EncodeError};
use faber_capabilities::{
    Capability, Command, Info, Reading, Timestamp, Twist2d, distance, estop, motor, planar_base,
};

/// The one-byte code a capability has in `READING`, `COMMAND` and `APPLIED` (protocol v1).
pub fn code(capability: Capability) -> Option<u8> {
    Some(match capability {
        Capability::Distance => 1,
        Capability::Motor => 2,
        Capability::PlanarBase => 3,
        Capability::Estop => 4,
        Capability::LaserScan | Capability::Pose | Capability::Position => return None,
    })
}

/// The capability with this code.
pub fn from_code(code: u8) -> Option<Capability> {
    Capability::ALL.iter().copied().find(|&c| self::code(c) == Some(code))
}

fn unsupported(capability: Capability) -> EncodeError {
    EncodeError::NoLayout { capability: capability.id() }
}

fn twist(w: &mut Writer, t: Twist2d) {
    w.f32(t.vx);
    w.f32(t.vy);
    w.f32(t.omega);
}

fn read_twist(r: &mut Reader) -> Result<Twist2d, DecodeError> {
    Ok(Twist2d::new(r.f32()?, r.f32()?, r.f32()?))
}

/// Reads a bit set and refuses bits that this version doesn't define.
fn flags(r: &mut Reader, known: u8, field: &'static str) -> Result<u8, DecodeError> {
    let f = r.u8()?;
    if f & !known != 0 { Err(DecodeError::BadValue { field }) } else { Ok(f) }
}

fn enum_index<T: Copy + PartialEq>(all: &[T], v: T) -> u8 {
    all.iter().position(|&x| x == v).expect("every variant is listed") as u8
}

fn enum_at<T: Copy>(all: &[T], i: u8, field: &'static str) -> Result<T, DecodeError> {
    all.get(i as usize).copied().ok_or(DecodeError::BadValue { field })
}

const DISTANCE_STATUS: &[distance::Status] = &[
    distance::Status::Ok,
    distance::Status::TooClose,
    distance::Status::TooFar,
    distance::Status::NoTarget,
    distance::Status::Error,
];
const ESTOP_SOURCE: &[estop::Source] = &[estop::Source::Part, estop::Source::App, estop::Source::Shell];

/// Writes a reading's layout (its `t_us` travels separately, as board milliseconds).
pub fn encode_reading(reading: &Reading, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let w = &mut Writer { out };
    match reading {
        Reading::Distance(r) => {
            w.f32(r.meters);
            w.u8(enum_index(DISTANCE_STATUS, r.status));
        }
        Reading::Motor(r) => {
            let present = r.velocity_rad_s.is_some() as u8
                | (r.position_rad.is_some() as u8) << 1
                | (r.current_a.is_some() as u8) << 2;
            w.u8(present);
            w.f32(r.velocity_rad_s.unwrap_or(0.0));
            w.f32(r.position_rad.unwrap_or(0.0));
            w.f32(r.current_a.unwrap_or(0.0));
        }
        Reading::PlanarBase(r) => {
            w.u8(r.measured.is_some() as u8 | (r.bumped as u8) << 1);
            twist(w, r.commanded);
            twist(w, r.measured.unwrap_or_default());
        }
        Reading::Estop(r) => {
            w.bool(r.engaged);
            w.u8(enum_index(ESTOP_SOURCE, r.source));
            w.str8(&r.reason, "reason")?;
        }
        other => return Err(unsupported(other.capability())),
    }
    Ok(())
}

/// Reads a reading's layout and stamps it with `t_us`.
pub fn decode_reading(capability: Capability, t_us: Timestamp, bytes: &[u8]) -> Result<Reading, DecodeError> {
    let r = &mut Reader::new(bytes);
    let reading = match capability {
        Capability::Distance => {
            let meters = r.f32()?;
            if !meters.is_finite() {
                return Err(DecodeError::BadValue { field: "meters" });
            }
            Reading::Distance(distance::Reading {
                t_us,
                meters,
                status: enum_at(DISTANCE_STATUS, r.u8()?, "status")?,
            })
        }
        Capability::Motor => {
            let present = flags(r, 0b111, "present")?;
            let (v, p, c) = (r.f32()?, r.f32()?, r.f32()?);
            Reading::Motor(motor::Reading {
                t_us,
                velocity_rad_s: (present & 1 != 0).then_some(v),
                position_rad: (present & 2 != 0).then_some(p),
                current_a: (present & 4 != 0).then_some(c),
            })
        }
        Capability::PlanarBase => {
            let f = flags(r, 0b11, "flags")?;
            let commanded = read_twist(r)?;
            let measured = read_twist(r)?;
            Reading::PlanarBase(planar_base::Reading {
                t_us,
                commanded,
                measured: (f & 1 != 0).then_some(measured),
                bumped: f & 2 != 0,
            })
        }
        Capability::Estop => Reading::Estop(estop::Reading {
            t_us,
            engaged: r.bool("engaged")?,
            source: enum_at(ESTOP_SOURCE, r.u8()?, "source")?,
            reason: r.str8("reason")?,
        }),
        other => return Err(DecodeError::NoLayout { capability: other.id() }),
    };
    r.end()?;
    Ok(reading)
}

/// Writes a command's layout: a kind byte, then its values.
pub fn encode_command(command: &Command, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let w = &mut Writer { out };
    match *command {
        Command::Motor(c) => {
            let (kind, value) = match c {
                motor::Command::SetVelocity { velocity_rad_s } => (0, velocity_rad_s),
                motor::Command::SetPosition { position_rad } => (1, position_rad),
                motor::Command::SetDuty { duty } => (2, duty),
            };
            w.u8(kind);
            w.f32(value);
        }
        Command::PlanarBase(planar_base::Command::SetVelocity(t)) => {
            w.u8(0);
            twist(w, t);
        }
    }
    Ok(())
}

pub fn decode_command(capability: Capability, bytes: &[u8]) -> Result<Command, DecodeError> {
    let r = &mut Reader::new(bytes);
    let command = match capability {
        Capability::Motor => {
            let kind = r.u8()?;
            let v = r.f32()?;
            Command::Motor(match kind {
                0 => motor::Command::SetVelocity { velocity_rad_s: v },
                1 => motor::Command::SetPosition { position_rad: v },
                2 => motor::Command::SetDuty { duty: v },
                _ => return Err(DecodeError::BadValue { field: "kind" }),
            })
        }
        Capability::PlanarBase => {
            if r.u8()? != 0 {
                return Err(DecodeError::BadValue { field: "kind" });
            }
            Command::PlanarBase(planar_base::Command::SetVelocity(read_twist(r)?))
        }
        other => return Err(DecodeError::NoLayout { capability: other.id() }),
    };
    r.end()?;
    Ok(command)
}

const MOTOR_KIND: &[motor::Kind] =
    &[motor::Kind::Dc, motor::Kind::Stepper, motor::Kind::Bldc, motor::Kind::ContinuousServo];
const DISTANCE_KIND: &[distance::Kind] =
    &[distance::Kind::Ultrasonic, distance::Kind::LaserTof, distance::Kind::Infrared, distance::Kind::Radar];

/// Writes a part's info layout, as sent in `PART`.
pub fn encode_info(info: &Info, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    let w = &mut Writer { out };
    match info {
        Info::Distance(i) => {
            w.f32(i.min_m);
            w.f32(i.max_m);
            w.f32(i.field_of_view_rad);
            w.u8(enum_index(DISTANCE_KIND, i.kind));
        }
        Info::Motor(i) => {
            w.u8(enum_index(MOTOR_KIND, i.kind));
            w.u8(i.modes.velocity as u8 | (i.modes.position as u8) << 1 | (i.modes.duty as u8) << 2);
            w.f32(i.max_velocity_rad_s.unwrap_or(f32::NAN));
        }
        Info::PlanarBase(i) => {
            w.bool(i.holonomic);
            twist(w, i.max_velocity);
            twist(w, i.max_acceleration);
            let n =
                u8::try_from(i.built_from.len()).map_err(|_| EncodeError::TooLong { field: "builtFrom" })?;
            w.u8(n);
            for name in &i.built_from {
                w.str8(name, "builtFrom")?;
            }
        }
        Info::Estop(i) => w.bool(i.hardware_latching),
        other => return Err(unsupported(other.capability())),
    }
    Ok(())
}

pub fn decode_info(capability: Capability, bytes: &[u8]) -> Result<Info, DecodeError> {
    let r = &mut Reader::new(bytes);
    let info = match capability {
        Capability::Distance => Info::Distance(distance::Info {
            min_m: r.f32()?,
            max_m: r.f32()?,
            field_of_view_rad: r.f32()?,
            kind: enum_at(DISTANCE_KIND, r.u8()?, "kind")?,
        }),
        Capability::Motor => {
            let kind = enum_at(MOTOR_KIND, r.u8()?, "kind")?;
            let m = flags(r, 0b111, "modes")?;
            let max = r.f32()?;
            Info::Motor(motor::Info {
                kind,
                modes: motor::Modes { velocity: m & 1 != 0, position: m & 2 != 0, duty: m & 4 != 0 },
                max_velocity_rad_s: (!max.is_nan()).then_some(max),
            })
        }
        Capability::PlanarBase => {
            let holonomic = r.bool("holonomic")?;
            let max_velocity = read_twist(r)?;
            let max_acceleration = read_twist(r)?;
            let n = r.u8()?;
            let built_from = (0..n).map(|_| r.str8("builtFrom")).collect::<Result<_, _>>()?;
            Info::PlanarBase(planar_base::Info { holonomic, max_velocity, max_acceleration, built_from })
        }
        Capability::Estop => Info::Estop(estop::Info { hardware_latching: r.bool("hardwareLatching")? }),
        other => return Err(DecodeError::NoLayout { capability: other.id() }),
    };
    r.end()?;
    Ok(info)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use proptest::prelude::*;

    pub fn finite() -> impl Strategy<Value = f32> {
        -1.0e6f32..1.0e6
    }

    fn twist() -> impl Strategy<Value = Twist2d> {
        (finite(), finite(), finite()).prop_map(|(a, b, c)| Twist2d::new(a, b, c))
    }

    fn name() -> impl Strategy<Value = String> {
        "[a-z0-9-]{0,20}"
    }

    /// Any reading that has a layout, with `t_us` as board milliseconds.
    pub fn reading() -> impl Strategy<Value = Reading> {
        let t = (0u64..u32::MAX as u64).prop_map(|ms| ms * 1000);
        prop_oneof![
            (t.clone(), finite(), 0..5u8).prop_map(|(t_us, meters, s)| Reading::Distance(
                distance::Reading { t_us, meters, status: DISTANCE_STATUS[s as usize] }
            )),
            (
                t.clone(),
                proptest::option::of(finite()),
                proptest::option::of(finite()),
                proptest::option::of(finite())
            )
                .prop_map(|(t_us, velocity_rad_s, position_rad, current_a)| Reading::Motor(
                    motor::Reading { t_us, velocity_rad_s, position_rad, current_a }
                )),
            (t.clone(), twist(), proptest::option::of(twist()), any::<bool>()).prop_map(
                |(t_us, commanded, measured, bumped)| Reading::PlanarBase(planar_base::Reading {
                    t_us,
                    commanded,
                    measured,
                    bumped
                })
            ),
            (t, any::<bool>(), 0..3u8, "[ -~]{0,60}").prop_map(|(t_us, engaged, s, reason)| Reading::Estop(
                estop::Reading { t_us, engaged, source: ESTOP_SOURCE[s as usize], reason }
            )),
        ]
    }

    pub fn command() -> impl Strategy<Value = Command> {
        prop_oneof![
            finite().prop_map(|v| Command::Motor(motor::Command::SetVelocity { velocity_rad_s: v })),
            finite().prop_map(|v| Command::Motor(motor::Command::SetPosition { position_rad: v })),
            finite().prop_map(|v| Command::Motor(motor::Command::SetDuty { duty: v })),
            twist().prop_map(|t| Command::PlanarBase(planar_base::Command::SetVelocity(t))),
        ]
    }

    pub fn info() -> impl Strategy<Value = Info> {
        prop_oneof![
            (finite(), finite(), finite(), 0..4u8).prop_map(|(min_m, max_m, field_of_view_rad, k)| {
                Info::Distance(distance::Info {
                    min_m,
                    max_m,
                    field_of_view_rad,
                    kind: DISTANCE_KIND[k as usize],
                })
            }),
            (0..4u8, any::<(bool, bool, bool)>(), proptest::option::of(finite())).prop_map(|(k, m, max)| {
                Info::Motor(motor::Info {
                    kind: MOTOR_KIND[k as usize],
                    modes: motor::Modes { velocity: m.0, position: m.1, duty: m.2 },
                    max_velocity_rad_s: max,
                })
            }),
            (any::<bool>(), twist(), twist(), proptest::collection::vec(name(), 0..4)).prop_map(
                |(holonomic, max_velocity, max_acceleration, built_from)| {
                    Info::PlanarBase(planar_base::Info {
                        holonomic,
                        max_velocity,
                        max_acceleration,
                        built_from,
                    })
                }
            ),
            any::<bool>().prop_map(|h| Info::Estop(estop::Info { hardware_latching: h })),
        ]
    }

    #[test]
    fn codes_round_trip_and_only_capabilities_with_a_layout_have_one() {
        for &c in Capability::ALL {
            if let Some(code) = code(c) {
                assert_eq!(from_code(code), Some(c));
            }
        }
        assert_eq!(from_code(0), None);
        assert_eq!(code(Capability::LaserScan), None);
    }

    #[test]
    fn the_layouts_are_exactly_as_documented() {
        let mut out = vec![];
        let r =
            Reading::Distance(distance::Reading { t_us: 0, meters: 1.5, status: distance::Status::TooFar });
        encode_reading(&r, &mut out).unwrap();
        assert_eq!(out, [0x00, 0x00, 0xC0, 0x3F, 2]);

        out.clear();
        let c = Command::Motor(motor::Command::SetDuty { duty: -0.5 });
        encode_command(&c, &mut out).unwrap();
        assert_eq!(out, [2, 0x00, 0x00, 0x00, 0xBF]);

        out.clear();
        let i = Info::Motor(motor::Info {
            kind: motor::Kind::Dc,
            modes: motor::Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: None,
        });
        encode_info(&i, &mut out).unwrap();
        assert_eq!(out, [0, 0b101, 0x00, 0x00, 0xC0, 0x7F]);

        out.clear();
        let m = Reading::Motor(motor::Reading {
            t_us: 0,
            velocity_rad_s: Some(2.0),
            position_rad: None,
            current_a: None,
        });
        encode_reading(&m, &mut out).unwrap();
        assert_eq!(out.len(), 13);
        assert_eq!(out[..5], [0b001, 0x00, 0x00, 0x00, 0x40]);
    }

    #[test]
    fn values_this_version_does_not_define_are_refused() {
        assert_eq!(
            decode_reading(Capability::Distance, 0, &[0, 0, 0, 0, 9]),
            Err(DecodeError::BadValue { field: "status" })
        );
        let nan = f32::NAN.to_le_bytes();
        assert_eq!(
            decode_reading(Capability::Distance, 0, &[nan[0], nan[1], nan[2], nan[3], 0]),
            Err(DecodeError::BadValue { field: "meters" })
        );
        assert_eq!(
            decode_command(Capability::Motor, &[3, 0, 0, 0, 0]),
            Err(DecodeError::BadValue { field: "kind" })
        );
        assert_eq!(
            decode_reading(Capability::Motor, 0, &[0b1000; 13]),
            Err(DecodeError::BadValue { field: "present" })
        );
        assert_eq!(
            decode_info(Capability::Estop, &[2]),
            Err(DecodeError::BadValue { field: "hardwareLatching" })
        );
        assert_eq!(decode_info(Capability::Estop, &[1, 0]), Err(DecodeError::TrailingBytes));
        assert_eq!(decode_command(Capability::Motor, &[0, 0]), Err(DecodeError::Truncated));
        assert_eq!(
            decode_command(Capability::Distance, &[]),
            Err(DecodeError::NoLayout { capability: "distance@1" })
        );
        let scan = faber_capabilities::laser_scan::Reading::even_bins(0, 0.0, 0.1, 0.1, vec![1.0]);
        assert_eq!(
            encode_reading(&Reading::LaserScan(scan), &mut vec![]),
            Err(EncodeError::NoLayout { capability: "laser-scan@1" })
        );
    }

    proptest! {
        #[test]
        fn readings_round_trip(r in reading()) {
            let mut out = vec![];
            encode_reading(&r, &mut out).unwrap();
            prop_assert_eq!(decode_reading(r.capability(), r.t_us(), &out).unwrap(), r);
        }

        #[test]
        fn commands_round_trip(c in command()) {
            let mut out = vec![];
            encode_command(&c, &mut out).unwrap();
            prop_assert_eq!(decode_command(c.capability(), &out).unwrap(), c);
        }

        #[test]
        fn infos_round_trip(i in info()) {
            let mut out = vec![];
            encode_info(&i, &mut out).unwrap();
            prop_assert_eq!(decode_info(i.capability(), &out).unwrap(), i);
        }

        #[test]
        fn garbage_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..64), c in 0..8u8) {
            if let Some(cap) = from_code(c) {
                let _ = decode_reading(cap, 0, &bytes);
                let _ = decode_command(cap, &bytes);
                let _ = decode_info(cap, &bytes);
            }
        }
    }
}
