//! The machine's motion: a commanded twist, limited by acceleration, integrated into the pose.
//! Kinematic only (D36): no forces, no wheel slip.

use super::geometry::{Footprint, Pose, Vec2, wrap_angle};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A velocity in the machine frame: `vx` forward and `vy` left (m/s), `omega` counter-clockwise
/// (rad/s). The same shape as `planar-base@1`'s twist2d.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct Twist {
    pub vx: f64,
    pub vy: f64,
    pub omega: f64,
}

impl Twist {
    pub const ZERO: Twist = Twist { vx: 0.0, vy: 0.0, omega: 0.0 };
}

/// How the simulated base moves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseSpec {
    pub footprint: Footprint,
    /// `false` for a differential drive: commanded `vy` is ignored.
    pub holonomic: bool,
    /// Largest change of the linear velocity (m/s²).
    pub max_accel: f64,
    /// Largest change of the turn rate (rad/s²).
    pub max_angular_accel: f64,
}

impl BaseSpec {
    /// A base with one acceleration limit, as `generic-planar-base`'s `max-accel` gives. The turn
    /// rate is limited so the footprint's corners accelerate no faster than `max_accel` either.
    pub fn new(footprint: Footprint, holonomic: bool, max_accel: f64) -> BaseSpec {
        let corner = Vec2::new(footprint.length / 2.0, footprint.width / 2.0).length();
        BaseSpec { footprint, holonomic, max_accel, max_angular_accel: max_accel / corner }
    }

    /// The velocity after `dt` seconds of chasing `command` from `current`.
    pub fn accelerate(&self, current: Twist, command: Twist, dt: f64) -> Twist {
        let target_vy = if self.holonomic { command.vy } else { 0.0 };
        let dv = Vec2::new(command.vx - current.vx, target_vy - current.vy);
        let max_dv = self.max_accel * dt;
        let dv = if dv.length() > max_dv { dv * (max_dv / dv.length()) } else { dv };
        let dw =
            (command.omega - current.omega).clamp(-self.max_angular_accel * dt, self.max_angular_accel * dt);
        Twist { vx: current.vx + dv.x, vy: current.vy + dv.y, omega: current.omega + dw }
    }
}

/// The pose after moving at `twist` for `dt` seconds. The linear velocity is applied along the
/// heading halfway through the step, which keeps arcs accurate at the 50 Hz step.
pub fn integrate(pose: Pose, twist: Twist, dt: f64) -> Pose {
    let mid_yaw = pose.yaw + twist.omega * dt / 2.0;
    let d = Vec2::new(twist.vx, twist.vy).rotated(mid_yaw) * dt;
    Pose { x: pose.x + d.x, y: pose.y + d.y, yaw: wrap_angle(pose.yaw + twist.omega * dt) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn spec(holonomic: bool) -> BaseSpec {
        BaseSpec::new(Footprint::SENNEP, holonomic, 1.0)
    }

    #[test]
    fn acceleration_is_limited_per_step() {
        let s = spec(true);
        let cmd = Twist { vx: 0.6, vy: 0.0, omega: 0.0 };
        let mut v = Twist::ZERO;
        let mut steps = 0;
        while v.vx < 0.6 {
            v = s.accelerate(v, cmd, 0.02);
            steps += 1;
        }
        assert_eq!(steps, 30, "0.6 m/s at 1 m/s² takes 0.6 s = 30 steps of 20 ms");
        assert!((v.vx - 0.6).abs() < 1e-12, "and stops at the command");
    }

    #[test]
    fn diagonal_acceleration_limits_the_vector_not_each_axis() {
        let v = spec(true).accelerate(Twist::ZERO, Twist { vx: 1.0, vy: 1.0, omega: 0.0 }, 0.1);
        assert!((Vec2::new(v.vx, v.vy).length() - 0.1).abs() < 1e-12);
        assert!((v.vx - v.vy).abs() < 1e-12);
    }

    #[test]
    fn turn_rate_is_limited_too() {
        let s = spec(true);
        assert!((s.max_angular_accel - 1.0 / 0.4_f64.hypot(0.325)).abs() < 1e-12);
        let v = s.accelerate(Twist::ZERO, Twist { omega: 5.0, ..Twist::ZERO }, 0.02);
        assert!((v.omega - s.max_angular_accel * 0.02).abs() < 1e-12);
    }

    #[test]
    fn non_holonomic_ignores_sideways_commands() {
        let v = spec(false).accelerate(Twist::ZERO, Twist { vx: 0.0, vy: 0.5, omega: 0.0 }, 1.0);
        assert_eq!(v, Twist::ZERO);
        let v = spec(true).accelerate(Twist::ZERO, Twist { vx: 0.0, vy: 0.5, omega: 0.0 }, 1.0);
        assert_eq!(v.vy, 0.5);
    }

    #[test]
    fn integrate_moves_in_the_machine_frame() {
        // Facing north, "left" is west.
        let p = integrate(Pose::new(0.0, 0.0, FRAC_PI_2), Twist { vx: 0.0, vy: 1.0, omega: 0.0 }, 0.5);
        assert!((p.x + 0.5).abs() < 1e-12 && p.y.abs() < 1e-12);
    }

    #[test]
    fn integrating_a_constant_turn_draws_a_circle() {
        // 1 m/s forward at 1 rad/s: a circle of radius 1 m, back home after 2π s.
        let mut p = Pose::default();
        let twist = Twist { vx: 1.0, vy: 0.0, omega: 1.0 };
        let n = 1000;
        for _ in 0..n {
            p = integrate(p, twist, 2.0 * PI / n as f64);
        }
        assert!(p.x.abs() < 1e-5 && p.y.abs() < 1e-5, "{p:?}");
        // Halfway round it is 2 m to the left.
        let mut q = Pose::default();
        for _ in 0..n / 2 {
            q = integrate(q, twist, 2.0 * PI / n as f64);
        }
        assert!(q.x.abs() < 1e-5 && (q.y - 2.0).abs() < 1e-5, "{q:?}");
    }
}
