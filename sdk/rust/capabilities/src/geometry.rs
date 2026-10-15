//! Small geometry types shared by several capabilities (`types` in wit/capabilities/types.wit).

use serde::{Deserialize, Serialize};

/// Planar velocity in the machine frame: `vx` forward and `vy` left (m/s), `omega`
/// counter-clockwise (rad/s).
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Twist2d {
    pub vx: f32,
    pub vy: f32,
    pub omega: f32,
}

impl Twist2d {
    pub const ZERO: Twist2d = Twist2d { vx: 0.0, vy: 0.0, omega: 0.0 };

    pub fn new(vx: f32, vy: f32, omega: f32) -> Twist2d {
        Twist2d { vx, vy, omega }
    }

    pub fn is_finite(&self) -> bool {
        self.vx.is_finite() && self.vy.is_finite() && self.omega.is_finite()
    }
}

#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A rotation as a unit quaternion.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Default for Quaternion {
    fn default() -> Self {
        Quaternion::IDENTITY
    }
}

impl Quaternion {
    pub const IDENTITY: Quaternion = Quaternion { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    /// A rotation of `yaw` rad about z (counter-clockwise seen from above).
    pub fn from_yaw(yaw: f32) -> Quaternion {
        let (s, c) = (yaw / 2.0).sin_cos();
        Quaternion { x: 0.0, y: 0.0, z: s, w: c }
    }

    /// The rotation about z, in -π..π.
    pub fn yaw(&self) -> f32 {
        let siny = 2.0 * (self.w * self.z + self.x * self.y);
        let cosy = 1.0 - 2.0 * (self.y * self.y + self.z * self.z);
        siny.atan2(cosy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_round_trips_through_a_quaternion() {
        for yaw in [0.0f32, 0.5, -1.0, 3.0] {
            assert!((Quaternion::from_yaw(yaw).yaw() - yaw).abs() < 1e-5, "{yaw}");
        }
        assert_eq!(Quaternion::default(), Quaternion::IDENTITY);
    }

    #[test]
    fn twist_json_shape() {
        let t = Twist2d::new(0.2, 0.0, -1.0);
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            serde_json::json!({"vx": 0.2f32, "vy": 0.0, "omega": -1.0})
        );
        assert!(t.is_finite());
        assert!(!Twist2d::new(f32::NAN, 0.0, 0.0).is_finite());
    }
}
