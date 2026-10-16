//! `obstacle@1`: something is (or isn't) in the way, from a sensor that only says yes or no
//! (docs/capabilities.md, "Ranging and motion"). Bumpers, whiskers and digital infrared obstacle
//! modules (FC-51, E18-D80NK) are binary spatial sensors: as `button@1` or `digital-in@1` an app
//! looking for "a button" would be offered a bumper, and as `distance@1` they would have no metres
//! (docs/research/common-parts.md §4.1). Every-message: each change is an event, and none may be
//! lost. Like every spatial part it has a `mount`, so the top view shows where it sits.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ObstacleReading"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// Something is in the way: the bumper is pressed, the module sees something.
    pub detected: bool,
}

/// How it senses.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ObstacleKind"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A switch that something presses: a bumper or a whisker.
    Bumper,
    /// A digital infrared module with a threshold (FC-51, KY-032, E18-D80NK).
    Infrared,
    /// A limit switch the machine runs into (a micro switch at the edge of where it may go).
    Limit,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[Kind::Bumper, Kind::Infrared, Kind::Limit];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Bumper => "bumper",
            Kind::Infrared => "infrared",
            Kind::Limit => "limit",
        }
    }
}

/// What an obstacle sensor is, reported once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ObstacleInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub kind: Kind,
    /// How close something must come before it is detected (m): about 0 for a bumper, the
    /// threshold set on an infrared module's potentiometer. `None` when it isn't known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub trigger_distance_m: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_and_info_round_trip() {
        let r = Reading { t_us: 3, detected: true };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v, serde_json::json!({"tUs": 3, "detected": true}));
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let i = Info { kind: Kind::Infrared, trigger_distance_m: Some(0.1) };
        let v = serde_json::to_value(&i).unwrap();
        assert_eq!(v, serde_json::json!({"kind": "infrared", "triggerDistanceM": 0.1f32}));
        assert_eq!(serde_json::from_value::<Info>(v).unwrap(), i);
        let bare = Info { kind: Kind::Bumper, trigger_distance_m: None };
        assert!(serde_json::to_value(&bare).unwrap().get("triggerDistanceM").is_none());
        for k in Kind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.id());
        }
    }
}
