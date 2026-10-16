//! `digital-in@1`: one digital input, such as a GPIO line (docs/capabilities.md, "GPIO level").
//! Every-message: each change of level is an event, and none may be lost.
//!
//! `level` is the logical level: `true` is *active*. A part set up as active-low reports a low
//! pin as `true`, so a button to ground reads `true` while it is pressed.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DigitalInReading"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    pub level: bool,
}

/// The pull resistor on an input (a part setting).
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "DigitalInPull"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pull {
    None,
    Up,
    Down,
}

impl Pull {
    pub const ALL: &'static [Pull] = &[Pull::None, Pull::Up, Pull::Down];

    pub fn id(self) -> &'static str {
        match self {
            Pull::None => "none",
            Pull::Up => "up",
            Pull::Down => "down",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_and_pull_round_trip() {
        let r = Reading { t_us: 3, level: true };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v, serde_json::json!({"tUs": 3, "level": true}));
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        for p in Pull::ALL {
            assert_eq!(serde_json::to_value(p).unwrap(), p.id());
        }
    }
}
