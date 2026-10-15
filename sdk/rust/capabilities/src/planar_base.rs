//! `planar-base@1` 🔒: a mobile base driven by a velocity in the machine frame
//! (docs/capabilities.md, "Actuation"). Leased, 250 ms; safe state stop.

use crate::{Clamped, Refused, Timestamp, Twist2d};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PlanarBaseReading"))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// The velocity last commanded (after clamping); zero in the safe state.
    pub commanded: Twist2d,
    /// The velocity the base really has, if it can tell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub measured: Option<Twist2d>,
    /// The base stopped short because it would have hit something (simulated worlds report it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<bool>", optional))]
    pub bumped: bool,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PlanarBaseCommand"))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    /// Drive at this velocity, in the machine frame, until the next command.
    SetVelocity(Twist2d),
}

impl Command {
    /// The safe state: stop.
    pub const STOP: Command = Command::SetVelocity(Twist2d::ZERO);

    pub fn kind(&self) -> &'static str {
        match self {
            Command::SetVelocity(_) => "set-velocity",
        }
    }

    /// Checks the command against the base's info and clamps it to its limits. A base that is
    /// not holonomic can't move sideways, so its `vy` is clamped to 0.
    pub fn clamp(self, info: &Info) -> Result<Clamped<Command>, Refused> {
        let Command::SetVelocity(t) = self;
        let max = info.max_velocity;
        let max_vy = if info.holonomic { max.vy } else { 0.0 };
        let vx = Clamped::of(t.vx, "vx", -max.vx, max.vx)?;
        let vy = Clamped::of(t.vy, "vy", -max_vy, max_vy)?;
        let omega = Clamped::of(t.omega, "omega", -max.omega, max.omega)?;
        Ok(Clamped {
            command: Command::SetVelocity(Twist2d::new(vx.command, vy.command, omega.command)),
            clamped: vx.clamped || vy.clamped || omega.clamped,
        })
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PlanarBaseInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Can move sideways (mecanum or omni wheels).
    pub holonomic: bool,
    /// The largest |vx|, |vy| (m/s) and |omega| (rad/s) it accepts.
    pub max_velocity: Twist2d,
    /// The largest change per second of each (m/s², rad/s²).
    pub max_acceleration: Twist2d,
    /// The parts this base is built from; a lease on the base locks them too.
    #[serde(default)]
    pub built_from: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(holonomic: bool) -> Info {
        Info {
            holonomic,
            max_velocity: Twist2d::new(0.6, 0.6, 1.5),
            max_acceleration: Twist2d::new(1.0, 1.0, 3.0),
            built_from: vec![],
        }
    }

    #[test]
    fn command_has_the_documented_shape() {
        let c: Command = serde_json::from_value(
            serde_json::json!({"kind": "set-velocity", "vx": 0.2, "vy": 0, "omega": 0}),
        )
        .unwrap();
        assert_eq!(c, Command::SetVelocity(Twist2d::new(0.2, 0.0, 0.0)));
        let v = serde_json::to_value(Command::STOP).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "set-velocity", "vx": 0.0, "vy": 0.0, "omega": 0.0}));
        assert_eq!(rmp_serde::from_slice::<Command>(&rmp_serde::to_vec_named(&c).unwrap()).unwrap(), c);
    }

    #[test]
    fn an_overshooting_joystick_is_clamped_not_refused() {
        let c = Command::SetVelocity(Twist2d::new(2.0, -0.1, -9.0)).clamp(&info(true)).unwrap();
        assert_eq!(c.command, Command::SetVelocity(Twist2d::new(0.6, -0.1, -1.5)));
        assert!(c.clamped);
        let c = Command::SetVelocity(Twist2d::new(0.1, 0.0, 0.2)).clamp(&info(true)).unwrap();
        assert!(!c.clamped);
    }

    #[test]
    fn a_base_that_is_not_holonomic_never_moves_sideways() {
        let c = Command::SetVelocity(Twist2d::new(0.1, 0.3, 0.0)).clamp(&info(false)).unwrap();
        assert_eq!(c.command, Command::SetVelocity(Twist2d::new(0.1, 0.0, 0.0)));
        assert!(c.clamped);
    }

    #[test]
    fn not_a_number_is_refused() {
        assert_eq!(
            Command::SetVelocity(Twist2d::new(0.0, 0.0, f32::INFINITY)).clamp(&info(true)),
            Err(Refused::NotFinite { field: "omega" })
        );
    }

    #[test]
    fn reading_leaves_out_what_it_does_not_know() {
        let r = Reading { t_us: 1, commanded: Twist2d::new(0.1, 0.0, 0.0), measured: None, bumped: false };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("measured").is_none() && v.get("bumped").is_none(), "{v}");
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let i = serde_json::to_value(info(true)).unwrap();
        assert_eq!(i["maxVelocity"]["omega"], 1.5);
        assert_eq!(serde_json::from_value::<Info>(i).unwrap(), info(true));
    }
}
