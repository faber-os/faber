//! Wall follower: an example Faber app (docs/apps.md). It drives a planar base along the wall on
//! its right, keeping the base's side 0.3 m from it, and publishes that distance as the derived
//! signal `app:wall-follower/wall-distance`.
//!
//! It never knows which LiDAR or which base it drives: it asks for a `laser-scan@1` role and a
//! `planar-base@1` role it may command, and the user picks the parts. The steering is a plain
//! function of one scan ([`steer`]), so it is tested on the PC without wasm or a core.

use faber_app_wasm::{App, Command, DriveError, Host, LogLevel, Reading};
use faber_capabilities::{Twist2d, distance, laser_scan, planar_base};
use std::f32::consts::PI;

/// How the app steers. The defaults fit SENNEP V2 (0.8 × 0.65 m, its LiDAR on the centre line).
/// Each can be changed in the app's state, e.g. `target-m = "0.25"`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    /// The gap to keep between the base's right side and the wall (m).
    pub target_m: f32,
    /// Half the base's width: from the LiDAR to the base's side (m).
    pub half_width_m: f32,
    /// From the LiDAR to the base's front (m).
    pub front_m: f32,
    /// Driving speed along the wall (m/s).
    pub speed: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Tuning { target_m: 0.3, half_width_m: 0.33, front_m: 0.2, speed: 0.25 }
    }
}

/// What one scan says to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Steer {
    /// The velocity to drive at (machine frame: x forward, y left).
    pub twist: Twist2d,
    /// The gap between the base's right side and the wall, when there is a wall on the right.
    pub gap_m: Option<f32>,
}

/// A wall further away than this isn't "the wall on the right": the app goes looking for one.
const NO_WALL_M: f32 = 1.5;
/// Stop and turn left when something ahead is closer than this to the base's front (m).
const BLOCKED_M: f32 = 0.45;

/// The shortest range within ±`half` of `angle` (rad, the LiDAR's frame, counter-clockwise).
fn range_near(scan: &laser_scan::Reading, angle: f32, half: f32) -> f32 {
    let mut best = f32::INFINITY;
    for i in 0..scan.ranges.len() {
        let mut d = scan.angle(i) - angle;
        while d > PI {
            d -= 2.0 * PI;
        }
        while d < -PI {
            d += 2.0 * PI;
        }
        let r = scan.ranges[i];
        if d.abs() <= half && r.is_finite() && r > 0.0 {
            best = best.min(r);
        }
    }
    best
}

/// Steers by one scan: drive ahead with the wall on the right, slide sideways (the base is
/// holonomic) to keep the gap, turn to stay parallel, and turn left at a corner.
pub fn steer(scan: &laser_scan::Reading, t: Tuning) -> Steer {
    let deg = PI / 180.0;
    let right = range_near(scan, -90.0 * deg, 5.0 * deg);
    let ahead = range_near(scan, 0.0, 25.0 * deg) - t.front_m;
    let gap = (right < NO_WALL_M).then_some(right - t.half_width_m);
    if ahead < BLOCKED_M {
        // A corner (or something in the way): turn left on the spot until it's clear.
        return Steer { twist: Twist2d::new(0.0, 0.0, 0.6), gap_m: gap };
    }
    let Some(g) = gap else {
        // No wall on the right yet: creep forward and slide right until one shows up.
        return Steer { twist: Twist2d::new(0.1, -0.15, 0.0), gap_m: None };
    };
    // Parallel to the wall, the rays 30° ahead and 30° behind the side are equally long. Turned
    // towards the wall, the one ahead is shorter: turn left (and the other way round).
    let (fore, aft) = (range_near(scan, -60.0 * deg, 3.0 * deg), range_near(scan, -120.0 * deg, 3.0 * deg));
    let omega = if fore.is_finite() && aft.is_finite() { (1.0 * (aft - fore)).clamp(-0.4, 0.4) } else { 0.0 };
    let vy = (-1.2 * (g - t.target_m)).clamp(-0.2, 0.2);
    Steer { twist: Twist2d::new(t.speed, vy, omega), gap_m: gap }
}

/// The app: steers on every new scan, and keeps driving (and so keeps its lease) between them.
#[derive(Default)]
pub struct WallFollower {
    tuning: Tuning,
    last: Option<Twist2d>,
    /// Said once that it can't drive, so the log isn't flooded.
    told: bool,
}

fn number(host: &mut dyn Host, key: &str, default: f32) -> f32 {
    host.state_get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
}

impl App for WallFollower {
    fn start(&mut self, host: &mut dyn Host, _now_us: u64) {
        let d = Tuning::default();
        self.tuning = Tuning {
            target_m: number(host, "target-m", d.target_m),
            half_width_m: number(host, "half-width-m", d.half_width_m),
            front_m: number(host, "front-m", d.front_m),
            speed: number(host, "speed", d.speed),
        };
        host.log(LogLevel::Info, &format!("following the wall on the right at {} m", self.tuning.target_m));
    }

    fn tick(&mut self, host: &mut dyn Host, _now_us: u64) {
        if let Some(Reading::LaserScan(scan)) = host.poll("lidar", 0) {
            let s = steer(&scan, self.tuning);
            self.last = Some(s.twist);
            let (meters, status) = match s.gap_m {
                Some(g) => (g.max(0.0), distance::Status::Ok),
                None => (NO_WALL_M, distance::Status::NoTarget),
            };
            host.publish(
                "wall-distance",
                Reading::Distance(distance::Reading { t_us: scan.t_us, meters, status }),
            );
        }
        let Some(twist) = self.last else { return };
        let go = Command::PlanarBase(planar_base::Command::SetVelocity(twist));
        let sent = match host.send("base", 0, go) {
            // No lease yet, or it ended (an operator took over, the e-stop, …): ask again. While
            // someone else drives, that is refused and the app just waits.
            Err(DriveError::LeaseRequired | DriveError::LeaseEnded { .. }) => {
                host.acquire("base", 0).and_then(|()| host.send("base", 0, go))
            }
            other => other,
        };
        match sent {
            Ok(_) => self.told = false,
            Err(e) if !self.told => {
                host.log(LogLevel::Warn, &format!("can't drive the base right now ({e:?}); waiting"));
                self.told = true;
            }
            Err(_) => {}
        }
    }

    fn stop(&mut self, host: &mut dyn Host) {
        host.release("base", 0);
    }
}

#[cfg(target_arch = "wasm32")]
faber_app_wasm::export!(WallFollower);

#[cfg(test)]
mod tests {
    use super::*;
    use faber_app_wasm::Role;
    use faber_app_wasm::testing::TestHost;

    /// A scan of a straight wall `d` metres to the right, parallel to the base, with nothing ahead.
    fn wall_on_right(d: f32, t_us: u64) -> laser_scan::Reading {
        let n = 450;
        let inc = 2.0 * PI / n as f32;
        let ranges = (0..n)
            .map(|i| {
                let a = -PI + i as f32 * inc;
                // The wall is the line y = -d: a ray at angle a (pointing right, sin a < 0) meets it.
                if a.sin() < -0.05 { d / -a.sin() } else { f32::INFINITY }
            })
            .collect();
        laser_scan::Reading::even_bins(t_us, -PI, inc, 0.1, ranges)
    }

    #[test]
    fn it_slides_towards_the_target_gap_and_drives_on() {
        let t = Tuning::default();
        let far = steer(&wall_on_right(0.33 + 0.5, 1), t);
        assert!(far.twist.vy < 0.0 && far.twist.vx > 0.0, "{far:?}");
        assert!((far.gap_m.unwrap() - 0.5).abs() < 0.02);
        let near = steer(&wall_on_right(0.33 + 0.1, 1), t);
        assert!(near.twist.vy > 0.0, "{near:?}");
        let right = steer(&wall_on_right(0.33 + 0.3, 1), t);
        assert!(right.twist.vy.abs() < 0.02 && right.twist.omega.abs() < 0.02, "{right:?}");
    }

    #[test]
    fn it_turns_left_at_a_corner_and_looks_for_a_wall() {
        let mut corner = wall_on_right(0.6, 1);
        for i in 0..corner.ranges.len() {
            if corner.angle(i).abs() < 0.3 {
                corner.ranges[i] = 0.5;
            }
        }
        let s = steer(&corner, Tuning::default());
        assert_eq!((s.twist.vx, s.twist.omega), (0.0, 0.6));
        let open = laser_scan::Reading::even_bins(1, -PI, 0.1, 0.1, vec![f32::INFINITY; 63]);
        let s = steer(&open, Tuning::default());
        assert!(s.gap_m.is_none() && s.twist.vy < 0.0);
    }

    #[test]
    fn the_app_drives_under_its_lease_publishes_and_asks_again_after_a_takeover() {
        let mut host = TestHost::with_roles(vec![
            Role { name: "lidar".into(), capability: "laser-scan@1".into(), command: false, parts: 1 },
            Role { name: "base".into(), capability: "planar-base@1".into(), command: true, parts: 1 },
        ]);
        let mut app = WallFollower::default();
        app.start(&mut host, 0);
        app.tick(&mut host, 1);
        assert!(host.sent.is_empty(), "nothing to steer by yet");
        host.set_reading("lidar", 0, Reading::LaserScan(wall_on_right(0.63, 5)));
        app.tick(&mut host, 2);
        app.tick(&mut host, 3);
        assert_eq!(host.sent.len(), 2, "it keeps sending between scans, which renews its lease");
        assert_eq!(host.published.len(), 1, "one scan, one wall distance");
        let Reading::Distance(d) = &host.published[0].1 else { panic!() };
        assert!((d.meters - 0.3).abs() < 0.02, "{d:?}");

        host.refuse = Some(DriveError::LeaseEnded { reason: "taken-over".into() });
        app.tick(&mut host, 4);
        assert_eq!(host.sent.len(), 2);
        assert!(host.logged.iter().any(|(_, m)| m.contains("waiting")), "{:?}", host.logged);
        host.refuse = None;
        app.tick(&mut host, 5);
        assert_eq!(host.sent.len(), 3, "it asked for the lease again and drives on");
        app.stop(&mut host);
        assert!(host.leases.is_empty());
    }

    #[test]
    fn the_tuning_comes_from_the_state() {
        let mut host = TestHost::default();
        host.state.insert("target-m".into(), "0.25".into());
        let mut app = WallFollower::default();
        app.start(&mut host, 0);
        assert_eq!(app.tuning, Tuning { target_m: 0.25, ..Tuning::default() });
    }
}
