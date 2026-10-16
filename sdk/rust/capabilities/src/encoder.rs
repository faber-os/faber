//! `encoder@1`: a quadrature encoder's count (docs/capabilities.md, "General inputs"): wheel
//! encoders on gear motors, rotary knobs. The count is in the encoder's own counts; when the part
//! knows how many counts make one turn ([`Info::counts_per_rev`]), the reading has the speed in
//! rad/s too.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "EncoderReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// Counts since the part started; positive is forward.
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub count: i64,
    /// Counts since the previous reading.
    pub delta: i32,
    /// The speed in counts per second.
    pub counts_per_s: f32,
    /// The speed of the shaft (rad/s), when the part knows its counts per turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub velocity_rad_s: Option<f32>,
}

/// What an encoder reports once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "EncoderInfo"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Counts for one turn of the shaft it measures, gearbox included. Not always whole (a 34:1
    /// gearbox is often 34.02:1). `None` when it isn't known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub counts_per_rev: Option<f32>,
}

impl Info {
    /// The shaft speed (rad/s) for a speed in counts per second, when the counts per turn are known.
    pub fn rad_s(&self, counts_per_s: f32) -> Option<f32> {
        self.counts_per_rev.filter(|c| *c > 0.0).map(|c| counts_per_s / c * std::f32::consts::TAU)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_and_info_round_trip() {
        let r =
            Reading { t_us: 5, count: -1234, delta: -12, counts_per_s: -120.0, velocity_rad_s: Some(-0.5) };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["countsPerS"], -120.0);
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let i = Info { counts_per_rev: None };
        assert!(serde_json::to_value(&i).unwrap().get("countsPerRev").is_none());
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }

    #[test]
    fn the_speed_in_rad_s_needs_the_counts_per_turn() {
        let i = Info { counts_per_rev: Some(1000.0) };
        assert!((i.rad_s(500.0).unwrap() - std::f32::consts::PI).abs() < 1e-5, "half a turn a second");
        assert_eq!(Info { counts_per_rev: None }.rad_s(500.0), None);
        assert_eq!(Info { counts_per_rev: Some(0.0) }.rad_s(500.0), None);
    }
}
