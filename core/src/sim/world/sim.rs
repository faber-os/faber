//! [`WorldSim`]: one world, one machine in it, and the clock the caller drives. This is what the
//! simulated parts (`sim::parts`) tick.

use super::geometry::Pose;
use super::kinematics::{BaseSpec, Twist, integrate};
use super::map::{MoverState, World};
use super::rng::Rng;
use super::sensors::{self, DistanceCone, DistanceReading, ScanSpec};
use faber_capabilities::laser_scan;
use faber_capabilities::{Mount, Timestamp};
use serde::{Deserialize, Serialize};

/// The fixed integration step (s): 50 Hz, as docs/simulation.md section 7 says.
pub const STEP_S: f64 = 0.02;

/// Halvings used to find how far a blocked step can go.
const BUMP_SEARCH: u32 = 12;

/// The machine after a [`WorldSim::step`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub pose: Pose,
    /// The velocity it moves at now (the command after the acceleration limit, or zero after a
    /// bump).
    pub velocity: Twist,
    /// `true` if something stopped it during this step: it stopped short at the last free pose.
    pub bumped: bool,
}

#[derive(Clone, Debug)]
pub struct WorldSim {
    world: World,
    base: BaseSpec,
    start: Pose,
    seed: u64,
    pose: Pose,
    velocity: Twist,
    /// World time (s): what the movers' positions depend on.
    time_s: f64,
    /// Time passed to `step` but not yet integrated (less than one `STEP_S`).
    pending_s: f64,
    rng: Rng,
}

impl WorldSim {
    /// A machine shaped and limited by `base`, standing still at `start` at time 0. `seed` fixes
    /// the sensor noise.
    pub fn new(world: World, base: BaseSpec, start: Pose, seed: u64) -> WorldSim {
        WorldSim {
            world,
            base,
            start,
            seed,
            pose: start,
            velocity: Twist::ZERO,
            time_s: 0.0,
            pending_s: 0.0,
            rng: Rng::new(seed),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn base(&self) -> &BaseSpec {
        &self.base
    }

    pub fn pose(&self) -> Pose {
        self.pose
    }

    pub fn velocity(&self) -> Twist {
        self.velocity
    }

    pub fn start(&self) -> Pose {
        self.start
    }

    /// World time (s) since the start or the last reset.
    pub fn time_s(&self) -> f64 {
        self.time_s
    }

    /// Back to the state `new` made: the machine at rest at the start pose, time 0, the noise
    /// sequence restarted. Everything that follows repeats exactly.
    pub fn reset(&mut self) {
        self.pose = self.start;
        self.velocity = Twist::ZERO;
        self.time_s = 0.0;
        self.pending_s = 0.0;
        self.rng = Rng::new(self.seed);
    }

    /// Like [`reset`](Self::reset), but from a new start pose, which later resets keep.
    pub fn reset_to(&mut self, start: Pose) {
        self.start = start;
        self.reset();
    }

    /// Whether the machine's footprint at `pose` would touch anything now.
    pub fn collides(&self, pose: Pose) -> bool {
        self.world.collides(pose, self.base.footprint, self.time_s)
    }

    /// Advances time by `dt_s` seconds with the base commanded to `command`, in fixed 20 ms
    /// steps (a remainder carries over to the next call). When a step would overlap anything the
    /// machine stops short at the last free pose along it, its velocity drops to zero, and the
    /// result says `bumped`. Something that walks into a machine standing still bumps it too.
    pub fn step(&mut self, dt_s: f64, command: Twist) -> Motion {
        let mut bumped = false;
        self.pending_s += dt_s.max(0.0);
        while self.pending_s >= STEP_S - 1e-12 {
            self.pending_s -= STEP_S;
            self.pending_s = self.pending_s.max(0.0);
            bumped |= self.fixed_step(command);
        }
        Motion { pose: self.pose, velocity: self.velocity, bumped }
    }

    /// One 20 ms step; `true` if it bumped.
    fn fixed_step(&mut self, command: Twist) -> bool {
        let velocity = self.base.accelerate(self.velocity, command, STEP_S);
        let t = self.time_s + STEP_S;
        self.time_s = t;
        let fp = self.base.footprint;
        let next = integrate(self.pose, velocity, STEP_S);
        if !self.world.collides(next, fp, t) {
            self.pose = next;
            self.velocity = velocity;
            return false;
        }
        // Blocked: find the farthest free fraction of this step. If even staying put collides
        // (something walked into the machine), stay put.
        let (mut free, mut blocked) = (0.0, 1.0);
        for _ in 0..BUMP_SEARCH {
            let mid = (free + blocked) / 2.0;
            if self.world.collides(integrate(self.pose, velocity, STEP_S * mid), fp, t) {
                blocked = mid;
            } else {
                free = mid;
            }
        }
        let last_free = integrate(self.pose, velocity, STEP_S * free);
        if free > 0.0 || !self.world.collides(last_free, fp, t) {
            self.pose = last_free;
        }
        self.velocity = Twist::ZERO;
        true
    }

    /// One LiDAR revolution from `mount`, now, stamped `t_us`.
    pub fn scan(&mut self, mount: &Mount, spec: &ScanSpec, t_us: Timestamp) -> laser_scan::Reading {
        sensors::scan(&self.world, self.time_s, self.pose, mount, spec, &mut self.rng, t_us)
    }

    /// A distance sensor's reading from `mount`, now.
    pub fn distance(&mut self, mount: &Mount, cone: &DistanceCone) -> DistanceReading {
        sensors::distance(&self.world, self.time_s, self.pose, mount, cone, &mut self.rng)
    }

    /// Where every mover is now.
    pub fn movers(&self) -> Vec<MoverState> {
        self.world.movers_at(self.time_s)
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::{Bounds, Footprint, Segment, Vec2};
    use super::super::map::{Geometry, Mover};
    use super::*;

    /// A 6 × 4 m room, x from 0 to 6, y from 0 to 4.
    fn room() -> Geometry {
        let c = [Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 4.0), Vec2::new(0.0, 4.0)];
        Geometry {
            bounds: Bounds { min: c[0], max: c[2] },
            walls: (0..4).map(|i| Segment::new(c[i], c[(i + 1) % 4])).collect(),
            ..Geometry::default()
        }
    }

    fn sim(geometry: Geometry, start: Pose) -> WorldSim {
        WorldSim::new(World::new(geometry), BaseSpec::new(Footprint::SENNEP, true, 1.0), start, 1)
    }

    const AHEAD: Twist = Twist { vx: 0.5, vy: 0.0, omega: 0.0 };

    #[test]
    fn drives_with_limited_acceleration_in_fixed_steps() {
        let mut s = sim(room(), Pose::new(1.0, 2.0, 0.0));
        // 1 s at 0.5 m/s commanded, 1 m/s² limit: 0.5 s ramping (0.125 m), then 0.5 s cruising (0.25 m).
        let mut m = s.step(0.0, AHEAD);
        for _ in 0..100 {
            m = s.step(0.01, AHEAD); // half steps: every second call integrates
        }
        assert!(!m.bumped);
        assert!((s.time_s() - 1.0).abs() < 1e-9);
        assert!((m.velocity.vx - 0.5).abs() < 1e-12);
        // Stepwise ramp: 25 steps at 0.02..0.5 m/s, then 25 at 0.5.
        let ramp: f64 = (1..=25).map(|k| 0.02 * f64::from(k) * STEP_S).sum();
        assert!((m.pose.x - (1.0 + ramp + 25.0 * 0.5 * STEP_S)).abs() < 1e-9, "{m:?}");
        assert_eq!(m.pose.y, 2.0);
    }

    #[test]
    fn stops_short_at_a_wall_and_reports_bumped() {
        let mut s = sim(room(), Pose::new(4.5, 2.0, 0.0));
        // The front edge starts at 4.9; the east wall is at 6.0.
        let mut bumped_at = None;
        for i in 0..400 {
            let m = s.step(STEP_S, AHEAD);
            if m.bumped {
                bumped_at = Some(i);
                break;
            }
        }
        assert!(bumped_at.is_some());
        let p = s.pose();
        assert!(p.x < 5.6 && p.x > 5.6 - 0.5 * STEP_S, "stopped short of the wall: {p:?}");
        assert!(!s.collides(p));
        assert_eq!(s.velocity(), Twist::ZERO);
        // Pushing on keeps it there and keeps reporting the bump; backing off works.
        let m = s.step(STEP_S, AHEAD);
        assert!(m.bumped && (m.pose.x - p.x).abs() < 1e-3);
        let m = s.step(0.2, Twist { vx: -0.5, ..Twist::ZERO });
        assert!(!m.bumped && m.pose.x < p.x);
    }

    #[test]
    fn a_turn_that_would_hit_is_stopped_too() {
        // 0.8 × 0.65 m facing east in a 0.75 m wide corridor fits; turned 90° it doesn't.
        let mut g = room();
        g.walls.push(Segment::new(Vec2::new(0.0, 0.75), Vec2::new(6.0, 0.75)));
        let mut s = sim(g, Pose::new(3.0, 0.375, 0.0));
        assert!(!s.collides(s.pose()));
        let m = s.step(1.0, Twist { omega: 1.0, ..Twist::ZERO });
        assert!(m.bumped);
        assert!(!s.collides(m.pose) && m.pose.yaw > 0.0 && m.pose.yaw < 0.2);
    }

    #[test]
    fn a_person_walking_into_it_bumps_it_and_it_stays_put() {
        let mut g = room();
        // Walks east along y = 2 from x = 0.5, reaching the machine's rear edge (x = 2.6) after ~1.85 s.
        g.movers.push(Mover {
            label: Some("person".into()),
            radius: 0.25,
            speed: 1.0,
            path: vec![Vec2::new(0.5, 2.0), Vec2::new(5.5, 2.0)],
            offset: 0.0,
        });
        let mut s = sim(g, Pose::new(3.0, 3.0, 0.0));
        s.reset_to(Pose::new(3.0, 2.0, 0.0));
        let mut first = None;
        for i in 0..200 {
            if s.step(STEP_S, Twist::ZERO).bumped {
                first = Some(i);
                break;
            }
        }
        let t = (first.unwrap() + 1) as f64 * STEP_S;
        assert!((t - 1.86).abs() < 0.03, "bumped at {t} s");
        assert_eq!(s.pose(), Pose::new(3.0, 2.0, 0.0));
        assert!((s.movers()[0].position - Vec2::new(0.5 + t, 2.0)).length() < 1e-9);
    }

    #[test]
    fn reset_repeats_everything_exactly() {
        let mut s = sim(room(), Pose::new(1.0, 2.0, 0.0));
        let mount = Mount::default();
        let run = |s: &mut WorldSim| {
            let m = s.step(0.5, Twist { vx: 0.3, vy: 0.1, omega: 0.2 });
            (m, s.scan(&mount, &ScanSpec::LD19, 0), s.distance(&mount, &DistanceCone::new(0.02, 4.0, 0.3)))
        };
        let first = run(&mut s);
        s.reset();
        assert_eq!(s.pose(), Pose::new(1.0, 2.0, 0.0));
        assert_eq!(s.time_s(), 0.0);
        assert_eq!(run(&mut s), first);
    }

    #[test]
    fn scan_and_distance_use_the_current_pose() {
        let mut s = sim(room(), Pose::new(3.0, 2.0, 0.0));
        let cone = DistanceCone { noise_m: 0.0, ..DistanceCone::new(0.02, 4.0, 0.0) };
        let front = Mount { x: 0.4, ..Mount::default() };
        assert!((s.distance(&front, &cone).meters - 2.6).abs() < 1e-6, "mounts are f32");
        s.step(2.0, AHEAD);
        assert!(s.distance(&front, &cone).meters < 2.0);
        let r = s.scan(&Mount::default(), &ScanSpec::LD19, 5);
        assert_eq!(r.len(), 450);
    }
}
