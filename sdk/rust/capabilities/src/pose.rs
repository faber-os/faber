//! `pose@1`: where something is and which way it faces (ROS PoseWithCovariance,
//! docs/capabilities.md). Usually a derived signal, e.g. `sennep:pose`, or the simulated
//! world's ground truth `world/pose`.

use crate::{Quaternion, Timestamp, Vec3};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[cfg_attr(feature = "typescript", ts(rename = "PoseReading"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[cfg_attr(feature = "typescript", ts(type = "number"))]
    pub t_us: Timestamp,
    /// The frame the pose is in: "map", "odom", "world", …
    pub frame: String,
    /// m
    pub position: Vec3,
    pub orientation: Quaternion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub position_variance: Option<Vec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub yaw_variance: Option<f32>,
}

impl Reading {
    /// A pose on the floor: `x`, `y` (m) and `yaw` (rad), with z = 0.
    pub fn planar(t_us: Timestamp, frame: &str, x: f32, y: f32, yaw: f32) -> Reading {
        Reading {
            t_us,
            frame: frame.into(),
            position: Vec3 { x, y, z: 0.0 },
            orientation: Quaternion::from_yaw(yaw),
            position_variance: None,
            yaw_variance: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planar_pose_round_trips() {
        let r = Reading::planar(4, "world", 1.0, 2.0, 0.5);
        assert!((r.orientation.yaw() - 0.5).abs() < 1e-5);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["frame"], "world");
        assert!(v.get("yawVariance").is_none());
        assert_eq!(serde_json::from_value::<Reading>(v).unwrap(), r);
        assert_eq!(rmp_serde::from_slice::<Reading>(&rmp_serde::to_vec_named(&r).unwrap()).unwrap(), r);
    }
}
