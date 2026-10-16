//! The app's safety layer: it limits the velocity the driver asks for so SENNEP never drives
//! into something it can see, and slows it on a critical battery.
//!
//! It only limits what this app sends. The core's e-stop, the lease's dead man's switch and the
//! board's own lease stay as they are, and anything else driving the base is not limited by it.

use crate::geometry::{Footprint, Point};
use faber_capabilities::Twist2d;

/// What the user set (the app's state), with SENNEP's defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// The fastest it drives, forward, back or sideways (m/s).
    pub max_speed: f32,
    /// The fastest it turns (rad/s).
    pub max_turn: f32,
    /// How close the leading edge may come to something (m).
    pub stop_m: f32,
    pub footprint: Footprint,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { max_speed: 0.3, max_turn: 0.8, stop_m: 0.35, footprint: Footprint::SENNEP }
    }
}

/// Approaching something, the speed towards it is at most this times the distance left to the
/// stop distance (1/s): 0.3 m/s at 0.2 m before it, 0 at it. So it slows down smoothly instead of
/// braking hard at the last moment.
pub const SLOW_PER_S: f32 = 1.5;
/// On a critical battery it drives at most this share of its top speed.
pub const CRITICAL_CAP: f32 = 0.3;
/// Room to keep around the circle the outline sweeps when turning on the spot (m).
pub const TURN_MARGIN_M: f32 = 0.05;

/// Which ways are blocked right now (an obstacle within the stop distance), whatever is asked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Blocked {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub turn: bool,
}

/// What the safety layer let through, and why it changed anything.
#[derive(Clone, Debug, PartialEq)]
pub struct Limited {
    pub twist: Twist2d,
    pub blocked: Blocked,
    /// The share of top speed allowed (1, or [`CRITICAL_CAP`] on a critical battery).
    pub cap: f32,
    /// Plain English, one sentence per reason, e.g. "Stopped: something 0.3 m ahead."
    pub reasons: Vec<String>,
}

/// The nearest gap along a unit direction, over every obstacle.
pub fn gap(points: &[Point], footprint: Footprint, dx: f32, dy: f32) -> Option<f32> {
    points.iter().filter_map(|p| footprint.gap_towards(*p, dx, dy)).min_by(f32::total_cmp)
}

fn allowed(gap: Option<f32>, stop_m: f32) -> f32 {
    gap.map_or(f32::INFINITY, |g| (SLOW_PER_S * (g - stop_m)).max(0.0))
}

/// Limits one axis' speed towards something `gap` away; says so when it did.
fn limit_axis(v: f32, gap: Option<f32>, stop_m: f32, where_: &str, reasons: &mut Vec<String>) -> f32 {
    let most = allowed(gap, stop_m);
    if v.abs() <= most {
        return v;
    }
    let g = gap.unwrap_or_default();
    reasons.push(if most == 0.0 {
        format!("Stopped: something {g:.1} m {where_}.")
    } else {
        format!("Slowing: something {g:.1} m {where_}.")
    });
    most.copysign(v)
}

/// The velocity to send for the one asked, given what is around (`points`, machine frame) and
/// whether the battery is critical.
pub fn limit(asked: Twist2d, points: &[Point], s: &Settings, battery_critical: bool) -> Limited {
    let f = s.footprint;
    let mut reasons = Vec::new();
    let cap = if battery_critical { CRITICAL_CAP } else { 1.0 };
    let mut t = Twist2d::new(
        asked.vx.clamp(-s.max_speed, s.max_speed) * cap,
        asked.vy.clamp(-s.max_speed, s.max_speed) * cap,
        asked.omega.clamp(-s.max_turn, s.max_turn) * cap,
    );
    if battery_critical && (asked.vx != 0.0 || asked.vy != 0.0 || asked.omega != 0.0) {
        reasons.push(format!("Battery critical: speed capped to {:.0} %.", CRITICAL_CAP * 100.0));
    }

    let (fwd, back) = (gap(points, f, 1.0, 0.0), gap(points, f, -1.0, 0.0));
    let (left, right) = (gap(points, f, 0.0, 1.0), gap(points, f, 0.0, -1.0));
    let near = |g: Option<f32>| g.is_some_and(|g| g <= s.stop_m);
    let sweep = f.turning_radius() + TURN_MARGIN_M;
    let closest_turning = points
        .iter()
        .filter(|p| !f.contains(**p))
        .map(|p| p.x.hypot(p.y))
        .filter(|d| *d < sweep)
        .min_by(f32::total_cmp);
    let blocked = Blocked {
        forward: near(fwd),
        back: near(back),
        left: near(left),
        right: near(right),
        turn: closest_turning.is_some(),
    };

    t.vx = if t.vx > 0.0 {
        limit_axis(t.vx, fwd, s.stop_m, "ahead", &mut reasons)
    } else {
        limit_axis(t.vx, back, s.stop_m, "behind", &mut reasons)
    };
    t.vy = if t.vy > 0.0 {
        limit_axis(t.vy, left, s.stop_m, "to the left", &mut reasons)
    } else {
        limit_axis(t.vy, right, s.stop_m, "to the right", &mut reasons)
    };
    // Each axis is fine on its own; a diagonal can still meet something off both axes (by a
    // corner). Check the direction it really goes.
    let speed = t.vx.hypot(t.vy);
    if speed > 1e-6 {
        let g = gap(points, f, t.vx / speed, t.vy / speed);
        let most = allowed(g, s.stop_m);
        if speed > most {
            let k = most / speed;
            t.vx *= k;
            t.vy *= k;
            let g = g.unwrap_or_default();
            reasons.push(if most == 0.0 {
                format!("Stopped: something {g:.1} m in the way.")
            } else {
                format!("Slowing: something {g:.1} m in the way.")
            });
        }
    }
    if t.omega != 0.0
        && let Some(d) = closest_turning
    {
        t.omega = 0.0;
        let clear = (d - f.turning_radius()).max(0.0);
        reasons.push(format!("Not turning: something {clear:.1} m from a corner."));
    }
    Limited { twist: t, blocked, cap, reasons }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_ahead(gap: f32) -> Vec<Point> {
        let x = 0.4 + gap;
        (-10..=10).map(|i| Point { x, y: i as f32 * 0.1 }).collect()
    }

    #[test]
    fn nothing_around_means_nothing_changes() {
        let s = Settings::default();
        let l = limit(Twist2d::new(0.3, -0.2, 0.5), &[], &s, false);
        assert_eq!(l.twist, Twist2d::new(0.3, -0.2, 0.5));
        assert_eq!((l.blocked, l.cap), (Blocked::default(), 1.0));
        assert!(l.reasons.is_empty());
        let fast = limit(Twist2d::new(2.0, 0.0, 9.0), &[], &s, false);
        assert_eq!(fast.twist, Twist2d::new(0.3, 0.0, 0.8), "never past the top speed");
    }

    #[test]
    fn it_stops_at_the_stop_distance_and_says_so() {
        let s = Settings::default();
        let at = limit(Twist2d::new(0.3, 0.0, 0.0), &wall_ahead(0.3), &s, false);
        assert_eq!(at.twist.vx, 0.0);
        assert!(at.blocked.forward && !at.blocked.back);
        assert_eq!(at.reasons, ["Stopped: something 0.3 m ahead."]);
        // Backing away, or sliding along it, is fine.
        let away = limit(Twist2d::new(-0.2, 0.1, 0.0), &wall_ahead(0.3), &s, false);
        assert_eq!(away.twist, Twist2d::new(-0.2, 0.1, 0.0));
        assert!(away.reasons.is_empty());
    }

    #[test]
    fn it_slows_down_before_it_gets_there() {
        let s = Settings::default();
        let far = limit(Twist2d::new(0.3, 0.0, 0.0), &wall_ahead(1.0), &s, false);
        assert_eq!(far.twist.vx, 0.3);
        let nearer = limit(Twist2d::new(0.3, 0.0, 0.0), &wall_ahead(0.47), &s, false);
        assert!((nearer.twist.vx - 0.18).abs() < 1e-4, "{nearer:?}");
        assert_eq!(nearer.reasons, ["Slowing: something 0.5 m ahead."]);
    }

    #[test]
    fn each_side_is_its_own_direction() {
        let s = Settings::default();
        let right_wall: Vec<Point> = (-5..=5).map(|i| Point { x: i as f32 * 0.1, y: -0.325 - 0.2 }).collect();
        let l = limit(Twist2d::new(0.2, -0.2, 0.0), &right_wall, &s, false);
        assert_eq!(l.twist, Twist2d::new(0.2, 0.0, 0.0), "it drives on along the wall, not into it");
        assert!(l.blocked.right && !l.blocked.forward);
        assert_eq!(l.reasons, ["Stopped: something 0.2 m to the right."]);
    }

    #[test]
    fn a_diagonal_into_a_corner_post_is_stopped() {
        let s = Settings::default();
        let post = [Point { x: 0.55, y: 0.45 }];
        let l = limit(Twist2d::new(0.2, 0.2, 0.0), &post, &s, false);
        assert_eq!((l.twist.vx, l.twist.vy), (0.0, 0.0), "{l:?}");
        assert!(l.reasons[0].ends_with("in the way."), "{l:?}");
    }

    #[test]
    fn it_wont_turn_with_something_by_a_corner() {
        let s = Settings::default();
        let l = limit(Twist2d::new(0.0, 0.0, 0.5), &[Point { x: 0.45, y: 0.3 }], &s, false);
        assert_eq!(l.twist.omega, 0.0);
        assert!(l.blocked.turn);
        assert_eq!(l.reasons, ["Not turning: something 0.0 m from a corner."]);
        let clear = limit(Twist2d::new(0.0, 0.0, 0.5), &wall_ahead(0.35), &s, false);
        assert_eq!(clear.twist.omega, 0.5, "a wall at the stop distance leaves room to turn");
    }

    #[test]
    fn a_critical_battery_caps_the_speed() {
        let s = Settings::default();
        let l = limit(Twist2d::new(0.3, 0.0, 0.8), &[], &s, true);
        assert!((l.twist.vx - 0.09).abs() < 1e-6 && (l.twist.omega - 0.24).abs() < 1e-6, "{l:?}");
        assert_eq!(l.cap, CRITICAL_CAP);
        assert_eq!(l.reasons, ["Battery critical: speed capped to 30 %."]);
        assert!(limit(Twist2d::ZERO, &[], &s, true).reasons.is_empty(), "nothing to cap");
    }

    #[test]
    fn its_own_body_is_not_an_obstacle() {
        let s = Settings::default();
        let inside = [Point { x: 0.3, y: 0.0 }, Point { x: -0.1, y: 0.2 }];
        let l = limit(Twist2d::new(0.3, 0.0, 0.5), &inside, &s, false);
        assert_eq!(l.twist, Twist2d::new(0.3, 0.0, 0.5));
    }
}
