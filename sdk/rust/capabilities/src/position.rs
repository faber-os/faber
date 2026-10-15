//! `position@1`: where on Earth the part is (ROS NavSatFix, docs/capabilities.md). Usually a GPS
//! module.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// How good the fix is. Names as in `wit/capabilities/ranging.wit` (`fix2d`, not `2d`: WIT names
/// can't start with a digit).
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PositionFix"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fix {
    /// No position yet: ignore the latitude and longitude.
    None,
    Fix2d,
    Fix3d,
    /// Corrected by a ground station (SBAS/DGPS).
    Dgps,
    RtkFloat,
    RtkFixed,
}

impl Fix {
    pub const ALL: &'static [Fix] =
        &[Fix::None, Fix::Fix2d, Fix::Fix3d, Fix::Dgps, Fix::RtkFloat, Fix::RtkFixed];

    pub fn id(self) -> &'static str {
        match self {
            Fix::None => "none",
            Fix::Fix2d => "fix2d",
            Fix::Fix3d => "fix3d",
            Fix::Dgps => "dgps",
            Fix::RtkFloat => "rtk-float",
            Fix::RtkFixed => "rtk-fixed",
        }
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PositionReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// Degrees, north positive (WGS 84). Meaningless while `fix` is `none`.
    pub latitude_deg: f64,
    /// Degrees, east positive (WGS 84). Meaningless while `fix` is `none`.
    pub longitude_deg: f64,
    /// Metres above mean sea level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub altitude_m: Option<f32>,
    pub fix: Fix,
    /// Satellites used for the fix.
    pub satellites: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub horizontal_accuracy_m: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub vertical_accuracy_m: Option<f32>,
    /// Speed over ground.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub speed_m_s: Option<f32>,
    /// Direction of travel over ground, clockwise from true north (rad).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub course_rad: Option<f32>,
}

impl Reading {
    /// No fix yet: zeros, and `fix` says so.
    pub fn no_fix(t_us: Timestamp, satellites: u8) -> Reading {
        Reading {
            t_us,
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            altitude_m: None,
            fix: Fix::None,
            satellites,
            horizontal_accuracy_m: None,
            vertical_accuracy_m: None,
            speed_m_s: None,
            course_rad: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading() -> Reading {
        Reading {
            latitude_deg: 55.676_098,
            longitude_deg: 12.568_337,
            altitude_m: Some(14.5),
            fix: Fix::Fix3d,
            satellites: 8,
            speed_m_s: Some(0.4),
            ..Reading::no_fix(3, 0)
        }
    }

    #[test]
    fn json_shape_round_trips() {
        let r = reading();
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["latitudeDeg"], 55.676_098, "a full f64: GPS needs more than 7 digits");
        assert_eq!(v["fix"], "fix3d");
        assert_eq!(v["speedMS"], serde_json::json!(0.4f32));
        assert!(v.get("courseRad").is_none(), "missing values are left out");
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }

    #[test]
    fn fixes_are_named_as_in_the_wit() {
        for f in Fix::ALL {
            assert_eq!(serde_json::to_value(f).unwrap(), f.id());
        }
        assert_eq!(Reading::no_fix(1, 4).fix, Fix::None);
    }
}
