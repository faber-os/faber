//! `estop@1`: the emergency stop (docs/capabilities.md, "Power and control"). Every-message:
//! no change may be lost. Engaging is always allowed; while engaged, the core revokes every lease.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// Who engaged or released it.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "EstopSource"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// A part, e.g. a hardware stop button.
    Part,
    /// A program using the API.
    App,
    /// Someone in the shell.
    Shell,
}

/// One change of the e-stop.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "EstopReading"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    pub engaged: bool,
    pub source: Source,
    /// Why, in the words of whoever did it.
    pub reason: String,
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "EstopInfo"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// A hardware stop that stays engaged until it is reset by hand.
    pub hardware_latching: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_round_trips() {
        let r = Reading { t_us: 9, engaged: true, source: Source::Shell, reason: "Stop button".into() };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"tUs": 9, "engaged": true, "source": "shell", "reason": "Stop button"})
        );
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
        let i = Info { hardware_latching: false };
        assert_eq!(serde_json::to_value(&i).unwrap(), serde_json::json!({"hardwareLatching": false}));
    }
}
