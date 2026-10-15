//! Plane geometry for the world: points, poses and frames, segments, rays and the machine's
//! footprint.
//!
//! Frames (docs/capabilities.md, REP-103): the **world** frame has x east and y north; the
//! **machine** frame has x forward and y left, its origin at the centre of the footprint; a
//! **mount** places a part in the machine frame. All lengths are metres, all angles radians,
//! counter-clockwise.

use faber_capabilities::Mount;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Neg, Sub};
use ts_rs::TS;

/// A point or a vector in the plane. Serialised as `[x, y]`, which keeps world files and the
/// geometry the shell receives short.
#[derive(Clone, Copy, Debug, Default, PartialEq, TS)]
#[ts(as = "[f64; 2]")]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

// By hand rather than `#[serde(from, into)]`, which ts-rs can't read.
impl Serialize for Vec2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        [self.x, self.y].serialize(s)
    }
}

impl<'de> Deserialize<'de> for Vec2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Vec2, D::Error> {
        <[f64; 2]>::deserialize(d).map(Vec2::from)
    }
}

impl From<[f64; 2]> for Vec2 {
    fn from([x, y]: [f64; 2]) -> Vec2 {
        Vec2 { x, y }
    }
}

impl From<Vec2> for [f64; 2] {
    fn from(v: Vec2) -> [f64; 2] {
        [v.x, v.y]
    }
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    /// The unit vector at `angle` from the x axis.
    pub fn from_angle(angle: f64) -> Vec2 {
        let (s, c) = angle.sin_cos();
        Vec2 { x: c, y: s }
    }

    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// The z component of the 3D cross product: positive when `o` is counter-clockwise of `self`.
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// Rotated counter-clockwise by `angle`.
    pub fn rotated(self, angle: f64) -> Vec2 {
        let (s, c) = angle.sin_cos();
        Vec2 { x: c * self.x - s * self.y, y: s * self.x + c * self.y }
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x + o.x, y: self.y + o.y }
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x - o.x, y: self.y - o.y }
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2 { x: self.x * k, y: self.y * k }
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}

/// A frame in the plane: where its origin is and which way its x axis points, relative to a
/// parent frame. The machine's pose is a `Pose` in the world; a mount is a `Pose` in the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub yaw: f64,
}

impl Pose {
    pub const fn new(x: f64, y: f64, yaw: f64) -> Pose {
        Pose { x, y, yaw }
    }

    pub fn position(self) -> Vec2 {
        Vec2 { x: self.x, y: self.y }
    }

    /// The unit vector of this frame's x axis, in the parent frame.
    pub fn heading(self) -> Vec2 {
        Vec2::from_angle(self.yaw)
    }

    /// A point given in this frame, expressed in the parent frame.
    pub fn to_parent(self, local: Vec2) -> Vec2 {
        self.position() + local.rotated(self.yaw)
    }

    /// A point given in the parent frame, expressed in this frame.
    pub fn to_local(self, parent: Vec2) -> Vec2 {
        (parent - self.position()).rotated(-self.yaw)
    }

    /// The frame `child` (given in this frame) expressed in the parent frame. With `self` the
    /// machine's pose in the world and `child` a mount, this is the part's pose in the world.
    pub fn compose(self, child: Pose) -> Pose {
        let p = self.to_parent(child.position());
        Pose { x: p.x, y: p.y, yaw: wrap_angle(self.yaw + child.yaw) }
    }
}

/// The part's frame in the machine frame. The world is 2D: `z`, `roll` and `pitch` are ignored,
/// so every part sees every obstacle whatever its height.
pub fn mount_pose(mount: &Mount) -> Pose {
    Pose { x: f64::from(mount.x), y: f64::from(mount.y), yaw: f64::from(mount.yaw) }
}

/// `angle` wrapped into (-π, π].
pub fn wrap_angle(angle: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let a = (angle + PI).rem_euclid(TAU) - PI;
    if a <= -PI { a + TAU } else { a }
}

/// A straight piece of wall or of an obstacle's outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Segment {
    pub a: Vec2,
    pub b: Vec2,
}

impl Segment {
    pub const fn new(a: Vec2, b: Vec2) -> Segment {
        Segment { a, b }
    }
}

/// How far a hit may lie past a segment's end and still count. Two walls meeting at a corner
/// share an endpoint; without this a ray through exactly that point could slip between them.
const END_SLACK: f64 = 1e-9;

/// The distance along the ray `origin + t · dir` (`dir` a unit vector) to `seg`, if it hits.
/// A ray running along the segment does not hit it; the segments at its ends will.
pub fn ray_segment(origin: Vec2, dir: Vec2, seg: &Segment) -> Option<f64> {
    let e = seg.b - seg.a;
    let denom = dir.cross(e);
    if denom.abs() < 1e-12 {
        return None;
    }
    let w = seg.a - origin;
    let t = w.cross(e) / denom;
    let u = w.cross(dir) / denom;
    (t >= 0.0 && (-END_SLACK..=1.0 + END_SLACK).contains(&u)).then_some(t)
}

/// The distance along the ray to a circle's edge, if it hits. A ray starting inside the circle
/// hits at 0: a sensor inside an obstacle sees nothing but that obstacle.
pub fn ray_circle(origin: Vec2, dir: Vec2, centre: Vec2, radius: f64) -> Option<f64> {
    let m = origin - centre;
    let c = m.dot(m) - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let b = m.dot(dir);
    if b > 0.0 {
        return None;
    }
    let disc = b * b - c;
    (disc >= 0.0).then(|| -b - disc.sqrt())
}

/// An axis-aligned rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}

impl Bounds {
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn contains(&self, p: Vec2) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }

    /// The part of `seg` inside this rectangle, as the parameter range `[t0, t1]` of
    /// `a + t · (b - a)`, if any (Liang–Barsky). Touching the edge counts.
    pub fn clip(&self, seg: &Segment) -> Option<(f64, f64)> {
        let d = seg.b - seg.a;
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        for (p, q) in [
            (-d.x, seg.a.x - self.min.x),
            (d.x, self.max.x - seg.a.x),
            (-d.y, seg.a.y - self.min.y),
            (d.y, self.max.y - seg.a.y),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
            }
        }
        (t0 <= t1).then_some((t0, t1))
    }
}

/// The machine's outline: a rectangle centred on the machine frame's origin, `length` along x
/// (forward) and `width` along y.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Footprint {
    pub length: f64,
    pub width: f64,
}

impl Footprint {
    /// SENNEP V2's footprint, the machine the prebuilt worlds are checked against.
    pub const SENNEP: Footprint = Footprint { length: 0.8, width: 0.65 };

    /// The rectangle in the machine frame.
    pub fn local_bounds(&self) -> Bounds {
        let h = Vec2::new(self.length / 2.0, self.width / 2.0);
        Bounds { min: -h, max: h }
    }

    /// Its four corners in the world, counter-clockwise from front-left.
    pub fn corners(&self, pose: Pose) -> [Vec2; 4] {
        let (l, w) = (self.length / 2.0, self.width / 2.0);
        [Vec2::new(l, w), Vec2::new(-l, w), Vec2::new(-l, -w), Vec2::new(l, -w)].map(|c| pose.to_parent(c))
    }

    /// The world-frame box around it at `pose`.
    pub fn world_bounds(&self, pose: Pose) -> Bounds {
        let c = self.corners(pose);
        let min = c.iter().fold(c[0], |m, p| Vec2::new(m.x.min(p.x), m.y.min(p.y)));
        let max = c.iter().fold(c[0], |m, p| Vec2::new(m.x.max(p.x), m.y.max(p.y)));
        Bounds { min, max }
    }

    /// Whether the footprint at `pose` touches or overlaps `seg`.
    pub fn hits_segment(&self, pose: Pose, seg: &Segment) -> bool {
        let local = Segment::new(pose.to_local(seg.a), pose.to_local(seg.b));
        self.local_bounds().clip(&local).is_some()
    }

    /// Whether the footprint at `pose` overlaps the circle.
    pub fn hits_circle(&self, pose: Pose, centre: Vec2, radius: f64) -> bool {
        let c = pose.to_local(centre);
        let b = self.local_bounds();
        let nearest = Vec2::new(c.x.clamp(b.min.x, b.max.x), c.y.clamp(b.min.y, b.max.y));
        (c - nearest).length() < radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-12
    }

    #[test]
    fn vec2_serialises_as_a_pair() {
        assert_eq!(serde_json::to_string(&Vec2::new(1.5, -2.0)).unwrap(), "[1.5,-2.0]");
        let v: Vec2 = serde_json::from_str("[3, 4]").unwrap();
        assert_eq!(v.length(), 5.0);
    }

    #[test]
    fn machine_frame_is_x_forward_y_left() {
        // Facing north: forward is +y in the world, left is -x (west).
        let pose = Pose::new(2.0, 3.0, FRAC_PI_2);
        assert!(close(pose.to_parent(Vec2::new(1.0, 0.0)), Vec2::new(2.0, 4.0)));
        assert!(close(pose.to_parent(Vec2::new(0.0, 1.0)), Vec2::new(1.0, 3.0)));
        assert!(close(pose.to_local(Vec2::new(2.0, 4.0)), Vec2::new(1.0, 0.0)));
    }

    #[test]
    fn mount_composes_onto_the_pose() {
        // A LiDAR 0.2 m forward, turned to look left, on a machine facing east at (1, 1).
        let mount = Mount { x: 0.2, z: 0.3, yaw: FRAC_PI_2 as f32, ..Mount::default() };
        let part = Pose::new(1.0, 1.0, 0.0).compose(mount_pose(&mount));
        assert!((part.x - 1.2).abs() < 1e-6 && (part.y - 1.0).abs() < 1e-12);
        assert!((part.yaw - FRAC_PI_2).abs() < 1e-6);
        // ...and the same machine facing west puts the LiDAR behind (1, 1) in the world.
        let part = Pose::new(1.0, 1.0, PI).compose(mount_pose(&mount));
        assert!((part.x - 0.8).abs() < 1e-6);
        assert!((wrap_angle(part.yaw) + FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn wrap_angle_lands_in_minus_pi_to_pi() {
        assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(-PI) - PI).abs() < 1e-12);
        assert!((wrap_angle(-FRAC_PI_2 - 2.0 * PI) + FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn ray_hits_segment_in_front_only() {
        let wall = Segment::new(Vec2::new(2.0, -1.0), Vec2::new(2.0, 1.0));
        assert_eq!(ray_segment(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), &wall), Some(2.0));
        assert_eq!(ray_segment(Vec2::new(0.0, 0.0), Vec2::new(-1.0, 0.0), &wall), None);
        assert_eq!(ray_segment(Vec2::new(0.0, 5.0), Vec2::new(1.0, 0.0), &wall), None);
        // Parallel rays never hit, even along the segment.
        assert_eq!(ray_segment(Vec2::new(2.0, -5.0), Vec2::new(0.0, 1.0), &wall), None);
    }

    #[test]
    fn ray_through_a_shared_corner_hits() {
        // Two walls meeting at (1, 1); the diagonal ray goes exactly through the corner.
        let a = Segment::new(Vec2::new(1.0, -1.0), Vec2::new(1.0, 1.0));
        let b = Segment::new(Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0));
        let dir = Vec2::from_angle(PI / 4.0);
        let hit = ray_segment(Vec2::default(), dir, &a).or_else(|| ray_segment(Vec2::default(), dir, &b));
        assert!((hit.unwrap() - 2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn ray_hits_circle_edge() {
        let t = ray_circle(Vec2::default(), Vec2::new(1.0, 0.0), Vec2::new(3.0, 0.0), 0.25).unwrap();
        assert!((t - 2.75).abs() < 1e-12);
        assert_eq!(ray_circle(Vec2::default(), Vec2::new(-1.0, 0.0), Vec2::new(3.0, 0.0), 0.25), None);
        assert_eq!(ray_circle(Vec2::default(), Vec2::new(0.0, 1.0), Vec2::new(3.0, 0.0), 0.25), None);
        assert_eq!(
            ray_circle(Vec2::new(3.1, 0.0), Vec2::new(1.0, 0.0), Vec2::new(3.0, 0.0), 0.25),
            Some(0.0)
        );
    }

    #[test]
    fn footprint_hits_segments_it_overlaps() {
        let fp = Footprint::SENNEP;
        let wall = Segment::new(Vec2::new(0.5, -5.0), Vec2::new(0.5, 5.0));
        // Front edge at x = 0.4 when facing east: clear by 0.1 m.
        assert!(!fp.hits_segment(Pose::new(0.0, 0.0, 0.0), &wall));
        assert!(fp.hits_segment(Pose::new(0.11, 0.0, 0.0), &wall));
        // Facing north the machine is only 0.325 m deep towards the wall.
        assert!(!fp.hits_segment(Pose::new(0.17, 0.0, FRAC_PI_2), &wall));
        assert!(fp.hits_segment(Pose::new(0.18, 0.0, FRAC_PI_2), &wall));
        // A segment entirely inside the footprint counts too.
        let crumb = Segment::new(Vec2::new(-0.1, 0.0), Vec2::new(0.1, 0.0));
        assert!(fp.hits_segment(Pose::default(), &crumb));
    }

    #[test]
    fn footprint_hits_circles_it_overlaps() {
        let fp = Footprint::SENNEP;
        assert!(!fp.hits_circle(Pose::default(), Vec2::new(0.7, 0.0), 0.25));
        assert!(fp.hits_circle(Pose::default(), Vec2::new(0.6, 0.0), 0.25));
        // Near a corner the distance is to the corner, not to the edges' lines.
        assert!(!fp.hits_circle(Pose::default(), Vec2::new(0.6, 0.53), 0.25));
    }

    #[test]
    fn footprint_bounds_grow_when_turned() {
        let b = Footprint::SENNEP.world_bounds(Pose::new(0.0, 0.0, PI / 4.0));
        // At 45° the corners reach (0.4 + 0.325) / √2 along both axes.
        let reach = (0.4 + 0.325) / 2f64.sqrt();
        assert!(close(b.max, Vec2::new(reach, reach)));
        assert!(close(b.min, -b.max));
    }
}
