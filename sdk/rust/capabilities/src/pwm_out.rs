//! `pwm-out@1` 🔒: one PWM output, for fans, single LEDs and vibration motors
//! (docs/capabilities.md, "GPIO level"). Leased, 1 s; the safe state is duty 0 (off).

use crate::{Clamped, Refused, Timestamp};
use serde::{Deserialize, Serialize};

/// The duty as last set.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PwmOutReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// 0 (off) to 1 (always on).
    pub duty: f32,
}

/// Leased commands.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PwmOutCommand"))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Command {
    SetDuty { duty: f32 },
}

impl Command {
    /// The safe state: off.
    pub const OFF: Command = Command::SetDuty { duty: 0.0 };

    pub fn kind(&self) -> &'static str {
        match self {
            Command::SetDuty { .. } => "set-duty",
        }
    }

    /// A duty is 0–1 on every part; beyond it is clamped, and a duty that is not a number refused.
    pub fn clamp(self) -> Result<Clamped<Command>, Refused> {
        match self {
            Command::SetDuty { duty } => {
                Ok(Clamped::of(duty, "duty", 0.0, 1.0)?.map(|d| Command::SetDuty { duty: d }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duty_is_clamped_to_0_1_and_must_be_a_number() {
        let c = Command::SetDuty { duty: 1.5 }.clamp().unwrap();
        assert_eq!(c, Clamped { command: Command::SetDuty { duty: 1.0 }, clamped: true });
        assert!(!Command::SetDuty { duty: 0.25 }.clamp().unwrap().clamped);
        assert_eq!(Command::SetDuty { duty: f32::NAN }.clamp(), Err(Refused::NotFinite { field: "duty" }));
    }

    #[test]
    fn commands_are_tagged_by_kind() {
        let v = serde_json::to_value(Command::OFF).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "set-duty", "duty": 0.0}));
        assert_eq!(serde_json::from_value::<Command>(v).unwrap(), Command::OFF);
        let r = Reading { t_us: 1, duty: 0.5 };
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }
}
