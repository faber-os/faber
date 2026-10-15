//! What a world is made of: walls, solid shapes and moving obstacles, and the [`World`] that
//! indexes them for raycasts and collision.

use super::geometry::{Bounds, Footprint, Pose, Segment, Vec2, ray_circle};
use super::grid::{DEFAULT_CELL, SegmentGrid};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A world's geometry in the world frame (metres, x east, y north). This is what
/// `GET /api/v0/world` sends the shell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    /// The area worth showing; everything solid lies inside it.
    pub bounds: Bounds,
    /// `true` if rays may leave the world without hitting anything (an open floor). A closed
    /// world returns every ray.
    #[serde(default)]
    pub open: bool,
    /// Wall pieces, drawn as lines.
    #[serde(default)]
    pub walls: Vec<Segment>,
    /// Solid things (furniture, boxes, legs), each a closed polygon, drawn filled.
    #[serde(default)]
    pub shapes: Vec<Shape>,
    /// Drawn only, never sensed or hit: table and desk tops, above the sensors and the machine
    /// (the world is 2D, so only their legs are solid).
    #[serde(default)]
    pub overhead: Vec<Shape>,
    /// Things that move by themselves, such as a person walking a loop.
    #[serde(default)]
    pub movers: Vec<Mover>,
}

/// A closed polygon; the last point connects back to the first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Shape {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    pub points: Vec<Vec2>,
}

impl Shape {
    pub fn edges(&self) -> impl Iterator<Item = Segment> + '_ {
        let n = self.points.len();
        (0..n).map(move |i| Segment::new(self.points[i], self.points[(i + 1) % n]))
    }

    /// Whether `p` lies inside (even–odd rule).
    pub fn contains(&self, p: Vec2) -> bool {
        let mut inside = false;
        for e in self.edges() {
            if (e.a.y > p.y) != (e.b.y > p.y) {
                let x = e.a.x + (p.y - e.a.y) / (e.b.y - e.a.y) * (e.b.x - e.a.x);
                if p.x < x {
                    inside = !inside;
                }
            }
        }
        inside
    }
}

/// A circle (a person, a pet) going round a closed path at a steady speed. Its position depends
/// only on the time, so the simulation stays repeatable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
pub struct Mover {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    /// Radius (m).
    pub radius: f64,
    /// Walking speed (m/s).
    pub speed: f64,
    /// Corners of the loop; the last connects back to the first.
    pub path: Vec<Vec2>,
    /// Where along the loop it is at time 0 (m).
    #[serde(default)]
    pub offset: f64,
}

impl Mover {
    fn loop_length(&self) -> f64 {
        let n = self.path.len();
        (0..n).map(|i| (self.path[(i + 1) % n] - self.path[i]).length()).sum()
    }

    /// Its centre at `t_s` seconds.
    pub fn position_at(&self, t_s: f64) -> Vec2 {
        let n = self.path.len();
        let total = self.loop_length();
        if n == 0 {
            return Vec2::default();
        }
        if total <= 0.0 {
            return self.path[0];
        }
        let mut s = (self.offset + self.speed * t_s).rem_euclid(total);
        for i in 0..n {
            let (a, b) = (self.path[i], self.path[(i + 1) % n]);
            let len = (b - a).length();
            if s <= len && len > 0.0 {
                return a + (b - a) * (s / len);
            }
            s -= len;
        }
        self.path[0]
    }
}

/// Where a mover is at some moment, for the shell's `world` topic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoverState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub position: Vec2,
    pub radius: f64,
}

/// A world's geometry, indexed for fast raycasts and collision checks.
#[derive(Clone, Debug)]
pub struct World {
    geometry: Geometry,
    grid: SegmentGrid,
}

impl World {
    pub fn new(geometry: Geometry) -> World {
        let segments: Vec<Segment> =
            geometry.walls.iter().copied().chain(geometry.shapes.iter().flat_map(Shape::edges)).collect();
        World { grid: SegmentGrid::new(segments, DEFAULT_CELL), geometry }
    }

    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    /// The distance along the ray `origin + t · dir` (`dir` a unit vector) to the first thing it
    /// hits within `max_t`, with the movers where they are at `t_s`.
    pub fn cast(&self, origin: Vec2, dir: Vec2, max_t: f64, t_s: f64) -> Option<f64> {
        let mut best = self.grid.cast(origin, dir, max_t).map(|h| h.t);
        for m in &self.geometry.movers {
            if let Some(t) = ray_circle(origin, dir, m.position_at(t_s), m.radius)
                && t <= max_t
                && best.is_none_or(|b| t < b)
            {
                best = Some(t);
            }
        }
        best
    }

    /// Whether a machine with `footprint` at `pose` touches anything at `t_s`.
    pub fn collides(&self, pose: Pose, footprint: Footprint, t_s: f64) -> bool {
        self.collides_static(pose, footprint)
            || self.geometry.movers.iter().any(|m| footprint.hits_circle(pose, m.position_at(t_s), m.radius))
    }

    /// Like [`collides`](Self::collides), ignoring the movers.
    pub fn collides_static(&self, pose: Pose, footprint: Footprint) -> bool {
        let area = footprint.world_bounds(pose);
        if self.grid.any_near(area, |s| footprint.hits_segment(pose, s)) {
            return true;
        }
        // A footprint can sit entirely inside a shape (spawned inside a box) without crossing an
        // edge.
        self.geometry.shapes.iter().any(|s| s.contains(pose.position()))
    }

    /// Every mover's position at `t_s`.
    pub fn movers_at(&self, t_s: f64) -> Vec<MoverState> {
        self.geometry
            .movers
            .iter()
            .map(|m| MoverState { label: m.label.clone(), position: m.position_at(t_s), radius: m.radius })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 4 m room with a 1 × 1 m box in its north-east corner.
    pub(crate) fn room() -> Geometry {
        let c = [Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(0.0, 4.0)];
        Geometry {
            bounds: Bounds { min: c[0], max: c[2] },
            open: false,
            walls: (0..4).map(|i| Segment::new(c[i], c[(i + 1) % 4])).collect(),
            shapes: vec![Shape {
                label: Some("box".into()),
                points: vec![
                    Vec2::new(3.0, 3.0),
                    Vec2::new(4.0, 3.0),
                    Vec2::new(4.0, 4.0),
                    Vec2::new(3.0, 4.0),
                ],
            }],
            overhead: Vec::new(),
            movers: Vec::new(),
        }
    }

    fn walker() -> Mover {
        // A 2 × 1 m rectangle loop: 6 m round at 1 m/s.
        Mover {
            label: Some("person".into()),
            radius: 0.25,
            speed: 1.0,
            path: vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), Vec2::new(2.0, 1.0), Vec2::new(0.0, 1.0)],
            offset: 0.0,
        }
    }

    #[test]
    fn mover_walks_its_loop_by_time() {
        let m = walker();
        assert_eq!(m.position_at(0.0), Vec2::new(0.0, 0.0));
        assert_eq!(m.position_at(1.5), Vec2::new(1.5, 0.0));
        assert_eq!(m.position_at(2.5), Vec2::new(2.0, 0.5));
        assert_eq!(m.position_at(5.5), Vec2::new(0.0, 0.5));
        assert_eq!(m.position_at(7.5), Vec2::new(1.5, 0.0), "loops after 6 s");
        let late = Mover { offset: 3.0, ..walker() };
        assert_eq!(late.position_at(0.0), Vec2::new(2.0, 1.0));
    }

    #[test]
    fn rays_hit_walls_shapes_and_movers() {
        let mut g = room();
        let w = World::new(g.clone());
        let o = Vec2::new(1.0, 1.0);
        assert_eq!(w.cast(o, Vec2::new(1.0, 0.0), f64::INFINITY, 0.0), Some(3.0));
        assert_eq!(w.cast(o, Vec2::new(1.0, 0.0), 2.0, 0.0), None, "capped at max_t");
        let diag = w.cast(o, Vec2::from_angle(std::f64::consts::FRAC_PI_4), f64::INFINITY, 0.0).unwrap();
        assert!((diag - 2.0 * 2f64.sqrt()).abs() < 1e-9, "stops at the box's corner, not the wall");
        // A person standing still at (2.5, 1): seen 1.25 m east.
        g.movers.push(Mover { path: vec![Vec2::new(2.5, 1.0)], ..walker() });
        let w = World::new(g);
        assert_eq!(w.cast(o, Vec2::new(1.0, 0.0), f64::INFINITY, 0.0), Some(1.25));
    }

    #[test]
    fn footprint_collides_with_walls_shapes_and_movers() {
        let fp = Footprint::SENNEP;
        let w = World::new(room());
        assert!(!w.collides(Pose::new(1.0, 1.0, 0.0), fp, 0.0));
        assert!(w.collides(Pose::new(0.3, 1.0, 0.0), fp, 0.0), "rear overlaps the west wall");
        assert!(w.collides(Pose::new(2.7, 2.8, 0.0), fp, 0.0), "corner overlaps the box");
        assert!(w.collides(Pose::new(3.5, 3.5, 0.0), fp, 0.0), "inside the box");
        let mut g = room();
        g.movers.push(walker());
        let w = World::new(g);
        // The person is at (1.5, 0) at 1.5 s, more than 0.25 m from a footprint around (1.5, 1.5).
        assert!(!w.collides(Pose::new(1.5, 1.5, 0.0), fp, 1.5));
        assert!(w.collides(Pose::new(1.5, 1.5, 0.0), fp, 3.5), "at 3.5 s the person is at (1.5, 1)");
        assert_eq!(w.movers_at(1.5)[0].position, Vec2::new(1.5, 0.0));
    }

    #[test]
    fn shape_contains_points_inside_only() {
        let s = &room().shapes[0];
        assert!(s.contains(Vec2::new(3.5, 3.5)));
        assert!(!s.contains(Vec2::new(2.5, 3.5)));
        assert_eq!(s.edges().count(), 4);
    }

    #[test]
    fn geometry_round_trips_as_json() {
        let mut g = room();
        g.movers.push(walker());
        let json = serde_json::to_value(&g).unwrap();
        assert_eq!(json["walls"][0], serde_json::json!({"a": [0.0, 0.0], "b": [4.0, 0.0]}));
        assert_eq!(json["shapes"][0]["label"], "box");
        let back: Geometry = serde_json::from_value(json).unwrap();
        assert_eq!(back, g);
    }
}
