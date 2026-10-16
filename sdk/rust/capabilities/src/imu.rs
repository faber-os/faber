//! `imu@1`: an inertial measurement unit (ROS Imu, docs/capabilities.md, "Ranging and motion"):
//! how fast it turns and how it is accelerated, in its own frame (its `mount` says how it sits on
//! the machine). An IMU at rest measures about +9.81 m/s² straight up: gravity is part of
//! `linear_acceleration`, as in ROS. `orientation` is only there when the part works it out
//! itself (sensor fusion, like a BNO055); an MPU6050 read raw has none.

use crate::{Quaternion, Timestamp, Vec3};
use serde::{Deserialize, Serialize};

/// Standard gravity (m/s²), the conventional value (CGPM 1901): what an IMU at rest reads on its
/// vertical axis.
pub const STANDARD_GRAVITY: f32 = 9.80665;

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "ImuReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub orientation: Option<Quaternion>,
    /// rad/s about x, y and z.
    pub angular_velocity: Vec3,
    /// m/s², gravity included.
    pub linear_acceleration: Vec3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub orientation_variance: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub angular_velocity_variance: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub linear_acceleration_variance: Option<Vec3>,
}

impl Reading {
    /// A reading with rates and accelerations only (an IMU without fusion or variances).
    pub fn raw(t_us: Timestamp, angular_velocity: Vec3, linear_acceleration: Vec3) -> Reading {
        Reading {
            t_us,
            orientation: None,
            angular_velocity,
            linear_acceleration,
            orientation_variance: None,
            angular_velocity_variance: None,
            linear_acceleration_variance: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_round_trips_and_leaves_out_what_it_hasnt() {
        let r =
            Reading::raw(4, Vec3 { x: 0.0, y: 0.0, z: 0.5 }, Vec3 { x: 0.0, y: 0.0, z: STANDARD_GRAVITY });
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("orientation").is_none());
        assert_eq!(v["angularVelocity"]["z"], 0.5);
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        let fused = Reading { orientation: Some(Quaternion::from_yaw(1.0)), ..r };
        assert_eq!(
            rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&fused).unwrap()).unwrap(),
            fused
        );
    }
}
