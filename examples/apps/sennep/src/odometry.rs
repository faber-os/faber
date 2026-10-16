//! Dead reckoning: where SENNEP is, from how fast its base says it moves. It starts at 0, 0,
//! facing 0 (wherever SENNEP was when the app started or the pose was reset), in a frame called
//! `odom`.
//!
//! It drifts: every small error in the wheels' speed (slip, a bump, a wrong wheel diameter) adds
//! up and is never corrected. Use it for "about how far did it go", not to navigate a room. In the
//! simulated world, `world/pose` is the true pose to compare it with.

use faber_capabilities::Twist2d;

/// The longest step it integrates at once (s): after a pause (the app was starved, or the
/// base's readings stopped) it doesn't jump by a stale velocity times a long time.
const MAX_STEP_S: f32 = 0.2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Odometry {
    pub x: f32,
    pub y: f32,
    /// Counter-clockwise from where it started (rad), kept within ±π.
    pub yaw: f32,
    last_us: Option<u64>,
}

impl Odometry {
    /// Moves on by `twist` (machine frame) from the last step until `now_us`.
    pub fn step(&mut self, twist: Twist2d, now_us: u64) {
        let dt = match self.last_us.replace(now_us) {
            Some(last) => (now_us.saturating_sub(last) as f32 / 1e6).min(MAX_STEP_S),
            None => return,
        };
        // Midpoint heading: exact for a steady turn's chord direction, near enough here.
        let mid = self.yaw + twist.omega * dt / 2.0;
        let (s, c) = mid.sin_cos();
        self.x += (twist.vx * c - twist.vy * s) * dt;
        self.y += (twist.vx * s + twist.vy * c) * dt;
        self.yaw = wrap(self.yaw + twist.omega * dt);
    }

    /// Back to 0, 0, facing 0, from now on.
    pub fn reset(&mut self) {
        *self = Odometry { last_us: self.last_us, ..Odometry::default() };
    }
}

fn wrap(a: f32) -> f32 {
    let two_pi = 2.0 * std::f32::consts::PI;
    let a = a % two_pi;
    if a > std::f32::consts::PI {
        a - two_pi
    } else if a < -std::f32::consts::PI {
        a + two_pi
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn run(o: &mut Odometry, twist: Twist2d, from_us: u64, secs: f32) -> u64 {
        let steps = (secs * 20.0) as u64;
        for i in 1..=steps {
            o.step(twist, from_us + i * 50_000);
        }
        from_us + steps * 50_000
    }

    #[test]
    fn it_integrates_straight_and_sideways() {
        let mut o = Odometry::default();
        o.step(Twist2d::ZERO, 0);
        let t = run(&mut o, Twist2d::new(0.5, 0.0, 0.0), 0, 2.0);
        assert!((o.x - 1.0).abs() < 1e-4 && o.y.abs() < 1e-6, "{o:?}");
        run(&mut o, Twist2d::new(0.0, 0.25, 0.0), t, 2.0);
        assert!((o.x - 1.0).abs() < 1e-4 && (o.y - 0.5).abs() < 1e-4, "{o:?}");
    }

    #[test]
    fn it_turns_and_then_drives_the_new_way() {
        let mut o = Odometry::default();
        o.step(Twist2d::ZERO, 0);
        let t = run(&mut o, Twist2d::new(0.0, 0.0, FRAC_PI_2), 0, 1.0);
        assert!((o.yaw - FRAC_PI_2).abs() < 1e-4, "{o:?}");
        run(&mut o, Twist2d::new(0.5, 0.0, 0.0), t, 2.0);
        assert!(o.x.abs() < 1e-3 && (o.y - 1.0).abs() < 1e-3, "now forward is +y: {o:?}");
        o.reset();
        assert_eq!((o.x, o.y, o.yaw), (0.0, 0.0, 0.0));
    }

    #[test]
    fn a_long_pause_is_not_a_long_drive() {
        let mut o = Odometry::default();
        o.step(Twist2d::new(0.5, 0.0, 0.0), 0);
        o.step(Twist2d::new(0.5, 0.0, 0.0), 10_000_000);
        assert!((o.x - 0.1).abs() < 1e-5, "{o:?}");
    }
}
