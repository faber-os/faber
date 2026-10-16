//! `servo@1` 🔒: a hobby servo that turns to an angle (docs/capabilities.md, "GPIO level").
//! Leased, 1 s. Its safe state is set up per part (D29): it *holds* its angle (the default), or it
//! is *released* (no more pulses, so it goes limp). The core asks for it with [`Command::Safe`],
//! and the part does what it is set up for.
//!
//! Most servos give no feedback: the reading is the angle it was last told, not a measurement.

use crate::{Clamped, Refused, Timestamp};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ServoReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// The angle it holds, as last commanded (rad). `None` while it is released: it gets no
    /// pulses, so nothing holds it, as when it has not been commanded yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub angle_rad: Option<f32>,
}

/// Leased commands.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ServoCommand"))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    /// Turn to `angle_rad`, within the part's range.
    #[serde(rename_all = "camelCase")]
    SetAngle { angle_rad: f32 },
    /// Stop the pulses: the servo goes limp.
    Release,
    /// Go to the safe state the part is set up for: hold the angle, or release.
    Safe,
}

impl Command {
    /// The safe state the core sends when a lease ends: what it means is the part's setting.
    pub const SAFE: Command = Command::Safe;

    pub fn kind(&self) -> &'static str {
        match self {
            Command::SetAngle { .. } => "set-angle",
            Command::Release => "release",
            Command::Safe => "safe",
        }
    }

    /// Clamps an angle to the servo's range; an angle that is not a number is refused.
    pub fn clamp(self, info: &Info) -> Result<Clamped<Command>, Refused> {
        match self {
            Command::SetAngle { angle_rad } => {
                let (lo, hi) =
                    (info.min_angle_rad.min(info.max_angle_rad), info.max_angle_rad.max(info.min_angle_rad));
                Ok(Clamped::of(angle_rad, "angleRad", lo, hi)?.map(|a| Command::SetAngle { angle_rad: a }))
            }
            other => Ok(Clamped { command: other, clamped: false }),
        }
    }
}

/// What a servo does when its lease ends (D29).
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ServoSafeState"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SafeState {
    /// Keep the angle it was last told (the default): an arm holding something doesn't drop it.
    Hold,
    /// Stop the pulses, so it goes limp.
    Release,
}

impl SafeState {
    pub const ALL: &'static [SafeState] = &[SafeState::Hold, SafeState::Release];

    pub fn id(self) -> &'static str {
        match self {
            SafeState::Hold => "hold",
            SafeState::Release => "release",
        }
    }
}

/// A servo's range and safe state, reported once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ServoInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// The angles it may be told (rad); the core clamps every command to them.
    pub min_angle_rad: f32,
    pub max_angle_rad: f32,
    pub safe_state: SafeState,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info { min_angle_rad: 0.0, max_angle_rad: std::f32::consts::PI, safe_state: SafeState::Hold }
    }

    #[test]
    fn commands_are_tagged_by_kind() {
        let c: Command =
            serde_json::from_value(serde_json::json!({"kind": "set-angle", "angleRad": 1.5})).unwrap();
        assert_eq!(c, Command::SetAngle { angle_rad: 1.5 });
        assert_eq!(c.kind(), "set-angle");
        for c in [Command::Release, Command::Safe] {
            let v = serde_json::to_value(c).unwrap();
            assert_eq!(v, serde_json::json!({"kind": c.kind()}));
            assert_eq!(serde_json::from_value::<Command>(v).unwrap(), c);
            assert_eq!(rmp_serde::from_slice::<Command>(&rmp_serde::to_vec_named(&c).unwrap()).unwrap(), c);
        }
    }

    #[test]
    fn an_angle_is_clamped_to_the_range_and_must_be_a_number() {
        let c = Command::SetAngle { angle_rad: 4.0 }.clamp(&info()).unwrap();
        assert_eq!(
            c,
            Clamped { command: Command::SetAngle { angle_rad: std::f32::consts::PI }, clamped: true }
        );
        assert!(!Command::SetAngle { angle_rad: 1.0 }.clamp(&info()).unwrap().clamped);
        assert_eq!(
            Command::SetAngle { angle_rad: f32::NAN }.clamp(&info()),
            Err(Refused::NotFinite { field: "angleRad" })
        );
        assert_eq!(Command::Release.clamp(&info()).unwrap().command, Command::Release);
    }

    #[test]
    fn readings_and_info_round_trip() {
        let r = Reading { t_us: 1, angle_rad: Some(0.5) };
        assert_eq!(serde_json::from_value::<Reading>(serde_json::to_value(&r).unwrap()).unwrap(), r);
        let released = Reading { t_us: 1, angle_rad: None };
        assert!(serde_json::to_value(&released).unwrap().get("angleRad").is_none());
        let v = serde_json::to_value(info()).unwrap();
        assert_eq!(v["safeState"], "hold");
        assert_eq!(serde_json::from_value::<Info>(v).unwrap(), info());
        for s in SafeState::ALL {
            assert_eq!(serde_json::to_value(s).unwrap(), s.id());
        }
    }
}
