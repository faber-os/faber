//! Where things are around the machine, in its own frame: x forward, y left, metres, angles
//! counter-clockwise from ahead (docs/capabilities.md). Plain functions over plain numbers, so
//! they are tested on the PC.

use faber_capabilities::{Mount, distance, laser_scan};

/// A point on the floor around the machine (m, machine frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// The machine's outline seen from above: a rectangle centred on the machine's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    /// Front to back (m).
    pub length: f32,
    /// Side to side (m).
    pub width: f32,
}

impl Footprint {
    /// SENNEP V2: 800 × 650 mm.
    pub const SENNEP: Footprint = Footprint { length: 0.8, width: 0.65 };

    fn half(&self) -> (f32, f32) {
        (self.length / 2.0, self.width / 2.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        let (hl, hw) = self.half();
        p.x.abs() <= hl && p.y.abs() <= hw
    }

    /// How far the machine can move along the unit direction `(dx, dy)` before its outline
    /// touches `p`: the gap between the edge that leads and the obstacle. `None` when it never
    /// touches it (it passes beside it, or it lies behind), and for a point inside the outline
    /// (a part of the machine itself, seen by its own LiDAR).
    pub fn gap_towards(&self, p: Point, dx: f32, dy: f32) -> Option<f32> {
        if self.contains(p) {
            return None;
        }
        let (hl, hw) = self.half();
        // The outline moved by s·d holds p when |p.x − s·dx| ≤ hl and |p.y − s·dy| ≤ hw.
        let slab = |p: f32, d: f32, h: f32| -> Option<(f32, f32)> {
            if d.abs() < 1e-6 {
                return (p.abs() <= h).then_some((f32::NEG_INFINITY, f32::INFINITY));
            }
            let (a, b) = ((p - h) / d, (p + h) / d);
            Some((a.min(b), a.max(b)))
        };
        let (x0, x1) = slab(p.x, dx, hl)?;
        let (y0, y1) = slab(p.y, dy, hw)?;
        let (enter, leave) = (x0.max(y0), x1.min(y1));
        (enter <= leave && enter >= 0.0).then_some(enter)
    }

    /// The radius the outline sweeps when it turns on the spot.
    pub fn turning_radius(&self) -> f32 {
        let (hl, hw) = self.half();
        hl.hypot(hw)
    }
}

/// A part's reading direction and position on the machine.
fn place(mount: Mount, range: f32, angle: f32) -> Point {
    let a = mount.yaw + angle;
    Point { x: mount.x + range * a.cos(), y: mount.y + range * a.sin() }
}

/// Every return of a scan, in the machine's frame (the LiDAR sits at `mount`). Rays with no
/// return are left out.
pub fn scan_points(scan: &laser_scan::Reading, mount: Mount) -> Vec<Point> {
    (0..scan.ranges.len())
        .filter_map(|i| {
            let r = scan.ranges[i];
            (r.is_finite() && r > 0.0).then(|| place(mount, r, scan.angle(i)))
        })
        .collect()
}

/// Where a distance sensor at `mount` sees something, if it does: along the way it looks. "Too
/// close" counts as something at its shortest range.
pub fn proximity_point(reading: &distance::Reading, mount: Mount) -> Option<Point> {
    let seen = matches!(reading.status, distance::Status::Ok | distance::Status::TooClose);
    (seen && reading.meters.is_finite() && reading.meters >= 0.0).then(|| place(mount, reading.meters, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

    const F: Footprint = Footprint::SENNEP;

    #[test]
    fn the_gap_is_from_the_leading_edge() {
        let ahead = Point { x: 0.4 + 0.35, y: 0.1 };
        assert!((F.gap_towards(ahead, 1.0, 0.0).unwrap() - 0.35).abs() < 1e-6);
        assert_eq!(F.gap_towards(ahead, -1.0, 0.0), None, "it lies behind when backing away");
        let left = Point { x: 0.0, y: 0.325 + 0.2 };
        assert!((F.gap_towards(left, 0.0, 1.0).unwrap() - 0.2).abs() < 1e-6);
        // Beside the path: driving ahead passes it.
        assert_eq!(F.gap_towards(Point { x: 1.0, y: 0.4 }, 1.0, 0.0), None);
        // Inside the outline: the machine itself.
        assert_eq!(F.gap_towards(Point { x: 0.1, y: 0.1 }, 1.0, 0.0), None);
    }

    #[test]
    fn a_diagonal_finds_a_point_off_both_axes() {
        // Off the front-left corner: neither straight ahead nor straight left meets it.
        let p = Point { x: 0.6, y: 0.5 };
        assert_eq!(F.gap_towards(p, 1.0, 0.0), None);
        assert_eq!(F.gap_towards(p, 0.0, 1.0), None);
        let d = FRAC_PI_4;
        let g = F.gap_towards(p, d.cos(), d.sin()).unwrap();
        assert!(g > 0.0 && g < 0.3, "{g}");
    }

    #[test]
    fn scans_and_sensors_land_in_the_machine_frame() {
        let lidar = Mount { x: 0.2, ..Mount::default() };
        let scan = laser_scan::Reading::even_bins(1, -PI, FRAC_PI_2, 0.1, vec![1.0, f32::INFINITY, 1.0, 0.5]);
        let pts = scan_points(&scan, lidar);
        assert_eq!(pts.len(), 3, "the ray with no return is left out");
        // −π: behind the LiDAR. 0: ahead. π/2: to its left.
        assert!((pts[0].x - (0.2 - 1.0)).abs() < 1e-5 && pts[0].y.abs() < 1e-5, "{pts:?}");
        assert!((pts[1].x - 1.2).abs() < 1e-5, "{pts:?}");
        assert!((pts[2].x - 0.2).abs() < 1e-5 && (pts[2].y - 0.5).abs() < 1e-5, "{pts:?}");

        let corner = Mount { x: 0.4, y: 0.325, yaw: FRAC_PI_4, ..Mount::default() };
        let seen = distance::Reading { t_us: 1, meters: 0.2, status: distance::Status::Ok };
        let p = proximity_point(&seen, corner).unwrap();
        assert!((p.x - (0.4 + 0.2 * FRAC_PI_4.cos())).abs() < 1e-5, "{p:?}");
        let nothing = distance::Reading { status: distance::Status::NoTarget, ..seen };
        assert_eq!(proximity_point(&nothing, corner), None);
        let close = distance::Reading { status: distance::Status::TooClose, meters: 0.02, ..seen };
        assert!(proximity_point(&close, corner).is_some(), "too close is something");
    }
}
