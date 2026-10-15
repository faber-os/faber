//! Simulated sensors: a scanning LiDAR (`laser-scan@1`) and a distance sensor's cone
//! (`distance@1`), both raycast from the part's mount. Noise comes from the caller's [`Rng`], so
//! a seed makes every reading repeatable.

use super::geometry::{Pose, Vec2, mount_pose};
use super::map::World;
use super::rng::Rng;
use faber_capabilities::laser_scan;
use faber_capabilities::{Mount, Timestamp};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// How a scanning LiDAR samples one revolution.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSpec {
    /// Points per revolution.
    pub points: usize,
    /// Nearer returns are dropped (m).
    pub range_min: f64,
    /// Farther returns are dropped (m).
    pub range_max: f64,
    /// Time of one revolution (s).
    pub scan_time: f64,
    /// Standard deviation of the range noise (m).
    pub noise_m: f64,
}

impl ScanSpec {
    /// The LDROBOT LD19 at its default 10 Hz: 0.8° between points (450 per turn), 0.02–12 m
    /// (docs/simulation.md, section 7), ±10 mm accuracy, taken here as a 1 cm deviation.
    pub const LD19: ScanSpec =
        ScanSpec { points: 450, range_min: 0.02, range_max: 12.0, scan_time: 0.1, noise_m: 0.01 };
}

/// A distance sensor's beam.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DistanceCone {
    /// Nearer targets read `too-close` (m).
    pub min_m: f64,
    /// Farther targets read `too-far` (m).
    pub max_m: f64,
    /// Full opening angle (rad); 0 is a single ray.
    pub fov_rad: f64,
    /// Rays spread evenly across the cone.
    pub rays: usize,
    /// Standard deviation of the noise (m).
    pub noise_m: f64,
}

impl DistanceCone {
    /// A cone with a ray every 2° or so, always an odd number so one ray runs down the middle,
    /// and 5 mm of noise.
    pub fn new(min_m: f64, max_m: f64, fov_rad: f64) -> DistanceCone {
        let rays = 1 + 2 * (fov_rad / 2.0 / 2f64.to_radians()).ceil().max(0.0) as usize;
        DistanceCone { min_m, max_m, fov_rad, rays, noise_m: 0.005 }
    }
}

/// `distance@1`'s status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DistanceStatus {
    Ok,
    /// Something is nearer than `min_m`.
    TooClose,
    /// The nearest thing is beyond `max_m`.
    TooFar,
    /// Nothing at all in the cone (only in an open world).
    NoTarget,
}

/// A distance reading. `meters` is the measured range when `ok`, `min_m` when `too-close`, and
/// `+inf` when `too-far` or `no-target`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistanceReading {
    pub meters: f64,
    pub status: DistanceStatus,
}

/// One revolution of a LiDAR at `mount` on a machine at `pose`, at world time `t_s`, stamped
/// `t_us`. Like the LD19's connector it gives raw points in the part's frame, turning clockwise
/// (decreasing angles in [0, 2π)) from a random phase, with `+inf` where nothing returns.
pub fn scan(
    world: &World,
    t_s: f64,
    pose: Pose,
    mount: &Mount,
    spec: &ScanSpec,
    rng: &mut Rng,
    t_us: Timestamp,
) -> laser_scan::Reading {
    let part = pose.compose(mount_pose(mount));
    let origin = part.position();
    let step = TAU / spec.points as f64;
    let phase = rng.uniform() * step;
    let mut angles = Vec::with_capacity(spec.points);
    let mut ranges = Vec::with_capacity(spec.points);
    for i in 0..spec.points {
        let angle = (-(phase + i as f64 * step)).rem_euclid(TAU);
        let hit = world.cast(origin, Vec2::from_angle(part.yaw + angle), spec.range_max + 1.0, t_s);
        let range = hit
            .map(|t| t + rng.gaussian(spec.noise_m))
            .filter(|r| (spec.range_min..=spec.range_max).contains(r))
            .map_or(f32::INFINITY, |r| r as f32);
        angles.push(angle as f32);
        ranges.push(range);
    }
    let mut r = laser_scan::Reading::raw_points(t_us, spec.scan_time as f32, angles, ranges);
    r.time_increment = Some((spec.scan_time / spec.points as f64) as f32);
    r
}

/// What a distance sensor at `mount` on a machine at `pose` reads at world time `t_s`: the
/// nearest hit of any ray in its cone.
pub fn distance(
    world: &World,
    t_s: f64,
    pose: Pose,
    mount: &Mount,
    cone: &DistanceCone,
    rng: &mut Rng,
) -> DistanceReading {
    let part = pose.compose(mount_pose(mount));
    let rays = cone.rays.max(1);
    let nearest = (0..rays)
        .filter_map(|i| {
            let a = if rays == 1 {
                0.0
            } else {
                -cone.fov_rad / 2.0 + cone.fov_rad * i as f64 / (rays - 1) as f64
            };
            world.cast(part.position(), Vec2::from_angle(part.yaw + a), f64::INFINITY, t_s)
        })
        .reduce(f64::min);
    let Some(d) = nearest else {
        return DistanceReading { meters: f64::INFINITY, status: DistanceStatus::NoTarget };
    };
    let d = (d + rng.gaussian(cone.noise_m)).max(0.0);
    if d < cone.min_m {
        DistanceReading { meters: cone.min_m, status: DistanceStatus::TooClose }
    } else if d > cone.max_m {
        DistanceReading { meters: f64::INFINITY, status: DistanceStatus::TooFar }
    } else {
        DistanceReading { meters: d, status: DistanceStatus::Ok }
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::{Bounds, Segment};
    use super::super::map::{Geometry, Mover};
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    /// A 10 × 6 m room, x from 0 to 10, y from 0 to 6.
    fn room(open: bool) -> World {
        let c = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 6.0), Vec2::new(0.0, 6.0)];
        let sides = if open { 3 } else { 4 }; // open: no west wall
        World::new(Geometry {
            bounds: Bounds { min: c[0], max: c[2] },
            open,
            walls: (0..sides).map(|i| Segment::new(c[i], c[(i + 1) % 4])).collect(),
            ..Geometry::default()
        })
    }

    fn quiet(spec: ScanSpec) -> ScanSpec {
        ScanSpec { noise_m: 0.0, ..spec }
    }

    /// The range of the point whose angle is nearest `angle`.
    fn range_at(r: &laser_scan::Reading, angle: f64) -> f32 {
        let i = (0..r.len())
            .min_by(|&a, &b| {
                let da = (f64::from(r.angle(a)) - angle)
                    .rem_euclid(TAU)
                    .min((angle - f64::from(r.angle(a))).rem_euclid(TAU));
                let db = (f64::from(r.angle(b)) - angle)
                    .rem_euclid(TAU)
                    .min((angle - f64::from(r.angle(b))).rem_euclid(TAU));
                da.total_cmp(&db)
            })
            .unwrap();
        r.ranges[i]
    }

    #[test]
    fn ld19_scan_has_its_resolution_and_raw_clockwise_angles() {
        let r = scan(
            &room(false),
            0.0,
            Pose::new(5.0, 3.0, 0.0),
            &Mount::default(),
            &ScanSpec::LD19,
            &mut Rng::new(1),
            7,
        );
        assert!(r.validate().is_ok());
        assert_eq!(r.len(), 450);
        assert_eq!(r.t_us, 7);
        assert!((r.scan_time - 0.1).abs() < 1e-6);
        let a = r.angles.as_ref().unwrap();
        assert!(a.iter().all(|x| (0.0..TAU as f32).contains(x)));
        // Clockwise: each angle is 0.8° below the last (modulo the wrap).
        let d = (f64::from(a[0]) - f64::from(a[1])).rem_euclid(TAU);
        assert!((d - 0.8_f64.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn scan_sees_walls_through_the_mount() {
        // Machine at (5, 3) facing north; LiDAR 0.2 m forward, turned to face the machine's left.
        let mount = Mount { x: 0.2, z: 0.3, yaw: FRAC_PI_2 as f32, ..Mount::default() };
        let r = scan(
            &room(false),
            0.0,
            Pose::new(5.0, 3.0, FRAC_PI_2),
            &mount,
            &quiet(ScanSpec::LD19),
            &mut Rng::new(1),
            0,
        );
        // The LiDAR sits at (5, 3.2) facing west: west wall 5 m ahead, north wall 2.8 m to its right.
        assert!((range_at(&r, 0.0) - 5.0).abs() < 1e-3);
        assert!((range_at(&r, -FRAC_PI_2) - 2.8).abs() < 1e-3);
        assert!((range_at(&r, PI) - 5.0).abs() < 1e-3);
        assert!((range_at(&r, FRAC_PI_2) - 3.2).abs() < 1e-3);
    }

    #[test]
    fn scan_drops_returns_out_of_range() {
        // 12 m maximum: from (0.5, 3) the east wall is 9.5 m away, but in a 20 m long hall it isn't.
        let spec = ScanSpec { range_max: 5.0, ..quiet(ScanSpec::LD19) };
        let r =
            scan(&room(false), 0.0, Pose::new(0.5, 3.0, 0.0), &Mount::default(), &spec, &mut Rng::new(1), 0);
        assert!(range_at(&r, 0.0).is_infinite(), "east wall beyond range_max");
        assert!((range_at(&r, PI) - 0.5).abs() < 1e-3);
        let spec = ScanSpec { range_min: 1.0, ..spec };
        let r =
            scan(&room(false), 0.0, Pose::new(0.5, 3.0, 0.0), &Mount::default(), &spec, &mut Rng::new(1), 0);
        assert!(range_at(&r, PI).is_infinite(), "west wall nearer than range_min");
    }

    #[test]
    fn scan_sees_a_moving_person_where_they_are_now() {
        let mut g = room(false).geometry().clone();
        g.movers.push(Mover {
            label: None,
            radius: 0.25,
            speed: 1.0,
            path: vec![Vec2::new(7.0, 1.0), Vec2::new(7.0, 5.0)],
            offset: 0.0,
        });
        let w = World::new(g);
        let spec = quiet(ScanSpec::LD19);
        // At 2 s the person is at (7, 3), straight east of the machine.
        let r = scan(&w, 2.0, Pose::new(5.0, 3.0, 0.0), &Mount::default(), &spec, &mut Rng::new(1), 0);
        assert!((range_at(&r, 0.0) - 1.75).abs() < 1e-3);
        let r = scan(&w, 0.0, Pose::new(5.0, 3.0, 0.0), &Mount::default(), &spec, &mut Rng::new(1), 0);
        assert!((range_at(&r, 0.0) - 5.0).abs() < 1e-3);
    }

    #[test]
    fn scan_is_repeatable_from_a_seed_and_noisy() {
        let w = room(false);
        let p = Pose::new(5.0, 3.0, 0.3);
        let a = scan(&w, 0.0, p, &Mount::default(), &ScanSpec::LD19, &mut Rng::new(9), 0);
        let b = scan(&w, 0.0, p, &Mount::default(), &ScanSpec::LD19, &mut Rng::new(9), 0);
        let c = scan(&w, 0.0, p, &Mount::default(), &ScanSpec::LD19, &mut Rng::new(10), 0);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let east = range_at(&a, -0.3);
        assert!(east != 5.0 && (east - 5.0).abs() < 0.05, "noise is small: {east}");
    }

    #[test]
    fn open_world_rays_escape_as_no_return() {
        let r = scan(
            &room(true),
            0.0,
            Pose::new(5.0, 3.0, 0.0),
            &Mount::default(),
            &quiet(ScanSpec::LD19),
            &mut Rng::new(1),
            0,
        );
        assert!(range_at(&r, PI).is_infinite());
        assert!((range_at(&r, 0.0) - 5.0).abs() < 1e-3);
    }

    #[test]
    fn distance_reports_the_nearest_in_its_cone() {
        let w = room(false);
        let cone = DistanceCone { noise_m: 0.0, ..DistanceCone::new(0.02, 4.0, 30f64.to_radians()) };
        assert!(cone.rays >= 15 && cone.rays % 2 == 1, "a ray down the middle");
        // Facing east from (8, 3): the wall is 2 m ahead.
        let d = distance(&w, 0.0, Pose::new(8.0, 3.0, 0.0), &Mount::default(), &cone, &mut Rng::new(1));
        assert_eq!(d.status, DistanceStatus::Ok);
        assert!((d.meters - 2.0).abs() < 1e-9, "{d:?}");
        // Facing east from (8, 0.5): the cone's right edge reaches the south wall first,
        // 0.5 / sin(15°) ≈ 1.93 m away.
        let d = distance(&w, 0.0, Pose::new(8.0, 0.5, 0.0), &Mount::default(), &cone, &mut Rng::new(1));
        assert!((d.meters - 0.5 / 15f64.to_radians().sin()).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn distance_reports_too_close_too_far_and_no_target() {
        let w = room(false);
        let cone = DistanceCone { noise_m: 0.0, ..DistanceCone::new(0.05, 2.0, 0.0) };
        assert_eq!(cone.rays, 1);
        let at =
            |x: f64| distance(&w, 0.0, Pose::new(x, 3.0, 0.0), &Mount::default(), &cone, &mut Rng::new(1));
        assert_eq!(at(9.97), DistanceReading { meters: 0.05, status: DistanceStatus::TooClose });
        assert_eq!(at(5.0), DistanceReading { meters: f64::INFINITY, status: DistanceStatus::TooFar });
        let open = room(true);
        let d = distance(&open, 0.0, Pose::new(5.0, 3.0, PI), &Mount::default(), &cone, &mut Rng::new(1));
        assert_eq!(d, DistanceReading { meters: f64::INFINITY, status: DistanceStatus::NoTarget });
        assert_eq!(serde_json::to_string(&DistanceStatus::TooClose).unwrap(), "\"too-close\"");
    }
}
