//! `motor@1` 🔒: one motor (docs/capabilities.md, "Actuation"). Leased, 250 ms; safe state
//! velocity 0.

use crate::{Clamped, Refused, Timestamp};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "MotorReading"))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub velocity_rad_s: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub position_rad: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub current_a: Option<f32>,
}

/// Leased commands. Which ones a motor accepts is in its [`Info::modes`].
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "MotorCommand"))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    #[serde(rename_all = "camelCase")]
    SetVelocity { velocity_rad_s: f32 },
    #[serde(rename_all = "camelCase")]
    SetPosition { position_rad: f32 },
    /// Open loop, -1..1.
    SetDuty { duty: f32 },
}

impl Command {
    /// The safe state: stopped.
    pub const STOP: Command = Command::SetVelocity { velocity_rad_s: 0.0 };

    pub fn kind(&self) -> &'static str {
        match self {
            Command::SetVelocity { .. } => "set-velocity",
            Command::SetPosition { .. } => "set-position",
            Command::SetDuty { .. } => "set-duty",
        }
    }

    /// Checks the command against the motor's info and clamps it to its limits.
    pub fn clamp(self, info: &Info) -> Result<Clamped<Command>, Refused> {
        let supported = match self {
            Command::SetVelocity { .. } => info.modes.velocity,
            Command::SetPosition { .. } => info.modes.position,
            Command::SetDuty { .. } => info.modes.duty,
        };
        if !supported {
            return Err(Refused::Unsupported { kind: self.kind() });
        }
        Ok(match self {
            Command::SetVelocity { velocity_rad_s: v } => {
                let max = info.max_velocity_rad_s.unwrap_or(f32::INFINITY);
                Clamped::of(v, "velocityRadS", -max, max)?.map(|v| Command::SetVelocity { velocity_rad_s: v })
            }
            Command::SetPosition { position_rad: p } => {
                Clamped::of(p, "positionRad", f32::NEG_INFINITY, f32::INFINITY)?
                    .map(|p| Command::SetPosition { position_rad: p })
            }
            Command::SetDuty { duty } => {
                Clamped::of(duty, "duty", -1.0, 1.0)?.map(|d| Command::SetDuty { duty: d })
            }
        })
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "MotorKind"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Dc,
    Stepper,
    Bldc,
    ContinuousServo,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[Kind::Dc, Kind::Stepper, Kind::Bldc, Kind::ContinuousServo];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Dc => "dc",
            Kind::Stepper => "stepper",
            Kind::Bldc => "bldc",
            Kind::ContinuousServo => "continuous-servo",
        }
    }
}

/// Which commands a motor accepts.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "MotorModes"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modes {
    pub velocity: bool,
    pub position: bool,
    pub duty: bool,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "MotorInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub kind: Kind,
    pub modes: Modes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub max_velocity_rad_s: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            kind: Kind::Dc,
            modes: Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: Some(10.0),
        }
    }

    #[test]
    fn commands_are_tagged_by_kind() {
        let c: Command =
            serde_json::from_value(serde_json::json!({"kind": "set-velocity", "velocityRadS": 2.5})).unwrap();
        assert_eq!(c, Command::SetVelocity { velocity_rad_s: 2.5 });
        let v = serde_json::to_value(Command::SetDuty { duty: -0.5 }).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "set-duty", "duty": -0.5}));
        let p = Command::SetPosition { position_rad: 1.0 };
        assert_eq!(serde_json::from_value::<Command>(serde_json::to_value(p).unwrap()).unwrap(), p);
        assert_eq!(rmp_serde::from_slice::<Command>(&rmp_serde::to_vec_named(&p).unwrap()).unwrap(), p);
    }

    #[test]
    fn clamps_to_the_info_and_says_so() {
        let c = Command::SetVelocity { velocity_rad_s: 25.0 }.clamp(&info()).unwrap();
        assert_eq!(c, Clamped { command: Command::SetVelocity { velocity_rad_s: 10.0 }, clamped: true });
        let c = Command::SetDuty { duty: 0.3 }.clamp(&info()).unwrap();
        assert!(!c.clamped);
        assert_eq!(
            Command::SetDuty { duty: -3.0 }.clamp(&info()).unwrap().command,
            Command::SetDuty { duty: -1.0 }
        );
    }

    #[test]
    fn refuses_what_the_motor_cannot_do_and_values_that_are_not_numbers() {
        assert_eq!(
            Command::SetPosition { position_rad: 1.0 }.clamp(&info()),
            Err(Refused::Unsupported { kind: "set-position" })
        );
        assert_eq!(
            Command::SetVelocity { velocity_rad_s: f32::NAN }.clamp(&info()),
            Err(Refused::NotFinite { field: "velocityRadS" })
        );
    }

    #[test]
    fn readings_and_info_round_trip() {
        let r = Reading { t_us: 3, velocity_rad_s: Some(1.0), position_rad: None, current_a: None };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("positionRad").is_none());
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let i = serde_json::to_value(info()).unwrap();
        assert_eq!(i["kind"], "dc");
        assert_eq!(serde_json::from_value::<Info>(i).unwrap(), info());
        for k in Kind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.id());
        }
    }
}
