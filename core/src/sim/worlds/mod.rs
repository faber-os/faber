//! The prebuilt worlds (docs/simulation.md, section 7), built in as TOML data files next to this
//! one. A file describes walls and furniture with a few helpers (walls with doorways, boxes,
//! furniture on legs, round posts); [`parse`] turns it into plain [`Geometry`].
//!
//! Adding a world: write `<name>.toml` here and add it to [`FILES`]. The tests check that it
//! parses, is closed, that the start is free for SENNEP's footprint and that the machine can
//! drive more than 3 m from it.

use crate::sim::world::{Bounds, Geometry, Mover, Pose, Segment, Shape, Vec2, World};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Every prebuilt world: its name and its file.
pub const FILES: &[(&str, &str)] = &[
    ("empty", include_str!("empty.toml")),
    ("apartment", include_str!("apartment.toml")),
    ("office", include_str!("office.toml")),
    ("warehouse", include_str!("warehouse.toml")),
    ("maze", include_str!("maze.toml")),
    ("obstacle-course", include_str!("obstacle-course.toml")),
];

/// A world ready to load: what `GET /api/v0/world` sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prebuilt {
    pub name: String,
    pub title: String,
    pub description: String,
    /// Where the machine starts unless `[world] start` in parts.toml says otherwise.
    pub start: Pose,
    pub geometry: Geometry,
}

impl Prebuilt {
    /// The geometry, indexed for the simulation.
    pub fn world(&self) -> World {
        World::new(self.geometry.clone())
    }

    pub fn info(&self) -> WorldInfo {
        WorldInfo {
            name: self.name.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            size: self.geometry.bounds.size(),
        }
    }
}

/// One entry of `GET /api/v0/worlds`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorldInfo {
    pub name: String,
    pub title: String,
    pub description: String,
    /// Width (east–west) and depth (north–south) of its bounds (m).
    pub size: Vec2,
}

/// The prebuilt world called `name`.
pub fn get(name: &str) -> Option<Prebuilt> {
    let (_, text) = FILES.iter().find(|(n, _)| *n == name)?;
    // The files are part of the binary and the tests parse every one of them.
    Some(parse(text).unwrap_or_else(|e| panic!("prebuilt world {name} is invalid: {e}")))
}

/// Every prebuilt world, in menu order.
pub fn list() -> Vec<WorldInfo> {
    FILES.iter().filter_map(|(name, _)| get(name)).map(|w| w.info()).collect()
}

/// Why a world file can't be used.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum WorldError {
    #[error("not a valid world file: {0}")]
    Syntax(String),
    #[error("{0}")]
    Invalid(String),
}

/// A world file as written. Every element kind is an array of tables (`[[wall]]`, `[[box]]` …).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    name: String,
    title: String,
    description: String,
    start: Pose,
    bounds: Bounds,
    #[serde(default)]
    open: bool,
    #[serde(default)]
    outline: Vec<Outline>,
    #[serde(default)]
    wall: Vec<Wall>,
    #[serde(default, rename = "box")]
    boxes: Vec<BoxDef>,
    #[serde(default)]
    polygon: Vec<Shape>,
    #[serde(default)]
    legs: Vec<Legs>,
    #[serde(default)]
    post: Vec<Post>,
    #[serde(default)]
    mover: Vec<Mover>,
}

/// A run of walls through `points`, closed back to the first unless `closed = false`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outline {
    points: Vec<Vec2>,
    #[serde(default = "yes")]
    closed: bool,
}

fn yes() -> bool {
    true
}

/// A straight wall from `from` to `to`, with doorways: `[offset, width]` pairs, metres along
/// the wall from `from`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wall {
    from: Vec2,
    to: Vec2,
    #[serde(default)]
    doors: Vec<[f64; 2]>,
}

/// A solid rectangle centred `at`, `size` = [along its x, along its y] before turning by `yaw`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoxDef {
    #[serde(default)]
    label: Option<String>,
    at: Vec2,
    size: Vec2,
    #[serde(default)]
    yaw: f64,
}

/// A table, desk or chair: only its four square legs (side `leg`) are solid, at the corners of
/// its `size`; the top is drawn but is above the sensors.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legs {
    #[serde(default)]
    label: Option<String>,
    at: Vec2,
    size: Vec2,
    #[serde(default)]
    yaw: f64,
    leg: f64,
}

/// A round post or bin, as a 16-sided polygon.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Post {
    #[serde(default)]
    label: Option<String>,
    at: Vec2,
    radius: f64,
}

/// The corners of a `size` rectangle centred `at`, turned by `yaw`, counter-clockwise.
fn rect(at: Vec2, size: Vec2, yaw: f64) -> Vec<Vec2> {
    let (x, y) = (size.x / 2.0, size.y / 2.0);
    let pose = Pose::new(at.x, at.y, yaw);
    [Vec2::new(-x, -y), Vec2::new(x, -y), Vec2::new(x, y), Vec2::new(-x, y)]
        .into_iter()
        .map(|c| pose.to_parent(c))
        .collect()
}

/// Parses and checks a world file.
pub fn parse(text: &str) -> Result<Prebuilt, WorldError> {
    let f: File = toml::from_str(text).map_err(|e| WorldError::Syntax(e.to_string()))?;
    let bad = |m: String| Err(WorldError::Invalid(m));
    let mut g = Geometry { bounds: f.bounds, open: f.open, ..Geometry::default() };

    for o in &f.outline {
        if o.points.len() < 2 {
            return bad("an outline needs at least 2 points".into());
        }
        let n = o.points.len();
        let pieces = if o.closed { n } else { n - 1 };
        g.walls.extend((0..pieces).map(|i| Segment::new(o.points[i], o.points[(i + 1) % n])));
    }
    for w in &f.wall {
        let d = w.to - w.from;
        let len = d.length();
        let mut doors = w.doors.clone();
        doors.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut at = 0.0;
        for [offset, width] in doors {
            if offset < at || width <= 0.0 || offset + width > len {
                return bad(format!(
                    "the wall from {:?} to {:?} has a doorway at {offset} m, {width} m wide, that \
                     overlaps another or doesn't fit in its {len:.2} m",
                    w.from, w.to
                ));
            }
            if offset > at {
                g.walls.push(Segment::new(w.from + d * (at / len), w.from + d * (offset / len)));
            }
            at = offset + width;
        }
        if at < len {
            g.walls.push(Segment::new(w.from + d * (at / len), w.to));
        }
    }
    for b in &f.boxes {
        if b.size.x <= 0.0 || b.size.y <= 0.0 {
            return bad(format!("the box at {:?} needs a positive size", b.at));
        }
        g.shapes.push(Shape { label: b.label.clone(), points: rect(b.at, b.size, b.yaw) });
    }
    for p in &f.polygon {
        if p.points.len() < 3 {
            return bad("a polygon needs at least 3 points".into());
        }
        g.shapes.push(p.clone());
    }
    for l in &f.legs {
        if l.leg <= 0.0 || l.size.x < 2.0 * l.leg || l.size.y < 2.0 * l.leg {
            return bad(format!("the legs at {:?} need a leg size that fits twice in its size", l.at));
        }
        let pose = Pose::new(l.at.x, l.at.y, l.yaw);
        let inset = Vec2::new(l.size.x / 2.0 - l.leg / 2.0, l.size.y / 2.0 - l.leg / 2.0);
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let centre = pose.to_parent(Vec2::new(sx * inset.x, sy * inset.y));
            g.shapes
                .push(Shape { label: l.label.clone(), points: rect(centre, Vec2::new(l.leg, l.leg), l.yaw) });
        }
        g.overhead.push(Shape { label: l.label.clone(), points: rect(l.at, l.size, l.yaw) });
    }
    for p in &f.post {
        if p.radius <= 0.0 {
            return bad(format!("the post at {:?} needs a positive radius", p.at));
        }
        let points =
            (0..16).map(|i| p.at + Vec2::from_angle(f64::from(i) * std::f64::consts::TAU / 16.0) * p.radius);
        g.shapes.push(Shape { label: p.label.clone(), points: points.collect() });
    }
    for m in &f.mover {
        if m.path.is_empty() || m.radius <= 0.0 || m.speed < 0.0 {
            return bad("a mover needs a path, a positive radius and a speed of 0 or more".into());
        }
    }
    g.movers = f.mover;

    let mut outside =
        g.walls.iter().flat_map(|s| [s.a, s.b]).chain(g.shapes.iter().flat_map(|s| s.points.clone()));
    if let Some(p) = outside.find(|p| !g.bounds.contains(*p)) {
        return bad(format!("{p:?} lies outside the bounds"));
    }
    if !g.bounds.contains(f.start.position()) {
        return bad("the start lies outside the bounds".into());
    }
    Ok(Prebuilt { name: f.name, title: f.title, description: f.description, start: f.start, geometry: g })
}

#[cfg(test)]
mod tests;
