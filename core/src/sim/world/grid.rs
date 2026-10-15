//! A uniform grid over the world's static segments, so a ray only tests the segments in the
//! cells it crosses (Amanatides–Woo traversal) and a footprint only the ones near it.

use super::geometry::{Bounds, Segment, Vec2, ray_segment};

/// Default cell edge (m). Rooms have walls every few metres and furniture in between, so half a
/// metre keeps a cell's list short without making long rays cross too many cells.
pub const DEFAULT_CELL: f64 = 0.5;

#[derive(Clone, Debug)]
pub struct SegmentGrid {
    segments: Vec<Segment>,
    bounds: Bounds,
    cell: f64,
    nx: usize,
    ny: usize,
    /// Cell `i` holds `indices[starts[i]..starts[i + 1]]`.
    starts: Vec<u32>,
    indices: Vec<u32>,
}

/// A ray's first hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Distance along the ray (m).
    pub t: f64,
    /// Index of the segment hit, as passed to [`SegmentGrid::new`].
    pub segment: usize,
}

impl SegmentGrid {
    pub fn new(segments: Vec<Segment>, cell: f64) -> SegmentGrid {
        assert!(cell > 0.0, "grid cell must be positive");
        let mut min = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for s in &segments {
            for p in [s.a, s.b] {
                min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
                max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
            }
        }
        if segments.is_empty() {
            (min, max) = (Vec2::default(), Vec2::default());
        }
        // One cell of margin, so nothing sits exactly on the grid's edge.
        let bounds = Bounds { min: min - Vec2::new(cell, cell), max: max + Vec2::new(cell, cell) };
        let size = bounds.size();
        let nx = (size.x / cell).ceil().max(1.0) as usize;
        let ny = (size.y / cell).ceil().max(1.0) as usize;

        let mut per_cell: Vec<Vec<u32>> = vec![Vec::new(); nx * ny];
        let pad = 1e-6;
        for (i, s) in segments.iter().enumerate() {
            let (cx0, cy0) = cell_of(&bounds, cell, nx, ny, Vec2::new(s.a.x.min(s.b.x), s.a.y.min(s.b.y)));
            let (cx1, cy1) = cell_of(&bounds, cell, nx, ny, Vec2::new(s.a.x.max(s.b.x), s.a.y.max(s.b.y)));
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    let lo = bounds.min + Vec2::new(cx as f64 * cell - pad, cy as f64 * cell - pad);
                    let hi = lo + Vec2::new(cell + 2.0 * pad, cell + 2.0 * pad);
                    if (Bounds { min: lo, max: hi }).clip(s).is_some() {
                        per_cell[cy * nx + cx].push(i as u32);
                    }
                }
            }
        }
        let mut starts = Vec::with_capacity(nx * ny + 1);
        let mut indices = Vec::new();
        starts.push(0);
        for c in per_cell {
            indices.extend(c);
            starts.push(indices.len() as u32);
        }
        SegmentGrid { segments, bounds, cell, nx, ny, starts, indices }
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    fn cell_segments(&self, cx: usize, cy: usize) -> &[u32] {
        let i = cy * self.nx + cx;
        &self.indices[self.starts[i] as usize..self.starts[i + 1] as usize]
    }

    /// The first segment the ray `origin + t · dir` (`dir` a unit vector) hits with
    /// `t <= max_t`, if any.
    pub fn cast(&self, origin: Vec2, dir: Vec2, max_t: f64) -> Option<Hit> {
        let (t_in, t_out) = self.clip_ray(origin, dir)?;
        let t_out = t_out.min(max_t);
        if t_in > t_out {
            return None;
        }
        let start = origin + dir * t_in;
        let (mut cx, mut cy) = cell_of(&self.bounds, self.cell, self.nx, self.ny, start);

        // Distance along the ray to the next vertical / horizontal cell boundary, and between two.
        let axis = |o: f64, d: f64, lo: f64, c: usize| -> (i64, f64, f64) {
            if d > 0.0 {
                (1, (lo + (c + 1) as f64 * self.cell - o) / d, self.cell / d)
            } else if d < 0.0 {
                (-1, (lo + c as f64 * self.cell - o) / d, -self.cell / d)
            } else {
                (0, f64::INFINITY, f64::INFINITY)
            }
        };
        let (step_x, mut next_x, delta_x) = axis(origin.x, dir.x, self.bounds.min.x, cx);
        let (step_y, mut next_y, delta_y) = axis(origin.y, dir.y, self.bounds.min.y, cy);

        let mut best: Option<Hit> = None;
        loop {
            for &i in self.cell_segments(cx, cy) {
                let i = i as usize;
                if let Some(t) = ray_segment(origin, dir, &self.segments[i])
                    && t <= t_out
                    && best.is_none_or(|b| t < b.t)
                {
                    best = Some(Hit { t, segment: i });
                }
            }
            let cell_exit = next_x.min(next_y);
            if best.is_some_and(|b| b.t <= cell_exit) || cell_exit > t_out {
                return best;
            }
            if next_x < next_y {
                let n = cx as i64 + step_x;
                if n < 0 || n >= self.nx as i64 {
                    return best;
                }
                cx = n as usize;
                next_x += delta_x;
            } else {
                let n = cy as i64 + step_y;
                if n < 0 || n >= self.ny as i64 {
                    return best;
                }
                cy = n as usize;
                next_y += delta_y;
            }
        }
    }

    /// The ray's parameter range inside the grid, starting no earlier than 0.
    fn clip_ray(&self, origin: Vec2, dir: Vec2) -> Option<(f64, f64)> {
        let (mut t0, mut t1) = (0.0_f64, f64::INFINITY);
        for (o, d, lo, hi) in [
            (origin.x, dir.x, self.bounds.min.x, self.bounds.max.x),
            (origin.y, dir.y, self.bounds.min.y, self.bounds.max.y),
        ] {
            if d == 0.0 {
                if o < lo || o > hi {
                    return None;
                }
            } else {
                let (a, b) = ((lo - o) / d, (hi - o) / d);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
        }
        (t0 <= t1).then_some((t0, t1))
    }

    /// Calls `f` with every segment that may touch `area` (some may not; none that do are missed).
    /// Stops early and returns `true` as soon as `f` does.
    pub fn any_near(&self, area: Bounds, mut f: impl FnMut(&Segment) -> bool) -> bool {
        if area.max.x < self.bounds.min.x
            || area.max.y < self.bounds.min.y
            || area.min.x > self.bounds.max.x
            || area.min.y > self.bounds.max.y
        {
            return false;
        }
        let (cx0, cy0) = cell_of(&self.bounds, self.cell, self.nx, self.ny, area.min);
        let (cx1, cy1) = cell_of(&self.bounds, self.cell, self.nx, self.ny, area.max);
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                if self.cell_segments(cx, cy).iter().any(|&i| f(&self.segments[i as usize])) {
                    return true;
                }
            }
        }
        false
    }
}

/// The cell containing `p`, clamped into the grid.
fn cell_of(bounds: &Bounds, cell: f64, nx: usize, ny: usize, p: Vec2) -> (usize, usize) {
    let fx = ((p.x - bounds.min.x) / cell).floor();
    let fy = ((p.y - bounds.min.y) / cell).floor();
    (fx.clamp(0.0, (nx - 1) as f64) as usize, fy.clamp(0.0, (ny - 1) as f64) as usize)
}

#[cfg(test)]
mod tests {
    use super::super::rng::Rng;
    use super::*;

    fn brute(segments: &[Segment], origin: Vec2, dir: Vec2, max_t: f64) -> Option<f64> {
        segments.iter().filter_map(|s| ray_segment(origin, dir, s)).filter(|t| *t <= max_t).reduce(f64::min)
    }

    fn random_segments(rng: &mut Rng, n: usize) -> Vec<Segment> {
        (0..n)
            .map(|_| {
                let a = Vec2::new(rng.uniform() * 20.0 - 10.0, rng.uniform() * 20.0 - 10.0);
                let d = Vec2::new(rng.uniform() * 4.0 - 2.0, rng.uniform() * 4.0 - 2.0);
                Segment::new(a, a + d)
            })
            .collect()
    }

    #[test]
    fn cast_agrees_with_testing_every_segment() {
        let mut rng = Rng::new(42);
        let segments = random_segments(&mut rng, 200);
        let grid = SegmentGrid::new(segments.clone(), 0.5);
        for _ in 0..5_000 {
            // Origins inside and outside the grid, any direction, some rays capped.
            let origin = Vec2::new(rng.uniform() * 30.0 - 15.0, rng.uniform() * 30.0 - 15.0);
            let dir = Vec2::from_angle(rng.uniform() * std::f64::consts::TAU);
            let max_t = if rng.uniform() < 0.5 { f64::INFINITY } else { rng.uniform() * 10.0 };
            let want = brute(&segments, origin, dir, max_t);
            let got = grid.cast(origin, dir, max_t).map(|h| h.t);
            assert_eq!(got, want, "origin {origin:?} dir {dir:?} max {max_t}");
        }
    }

    #[test]
    fn axis_aligned_rays_and_rays_along_cell_edges_work() {
        let wall = Segment::new(Vec2::new(3.0, -2.0), Vec2::new(3.0, 2.0));
        let floor = Segment::new(Vec2::new(-2.0, -1.0), Vec2::new(2.0, -1.0));
        let grid = SegmentGrid::new(vec![wall, floor], 0.5);
        assert_eq!(grid.cast(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), f64::INFINITY).unwrap().t, 3.0);
        assert_eq!(grid.cast(Vec2::new(0.0, 0.5), Vec2::new(1.0, 0.0), f64::INFINITY).unwrap().segment, 0);
        assert_eq!(grid.cast(Vec2::new(0.0, 0.0), Vec2::new(0.0, -1.0), f64::INFINITY).unwrap().t, 1.0);
        assert_eq!(grid.cast(Vec2::new(0.0, 0.0), Vec2::new(-1.0, 0.0), f64::INFINITY), None);
        assert_eq!(grid.cast(Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), 2.5), None);
    }

    #[test]
    fn any_near_finds_segments_in_the_area() {
        let wall = Segment::new(Vec2::new(3.0, -2.0), Vec2::new(3.0, 2.0));
        let grid = SegmentGrid::new(vec![wall], 0.5);
        let near = |x: f64| Bounds { min: Vec2::new(x - 0.1, -0.1), max: Vec2::new(x + 0.1, 0.1) };
        assert!(grid.any_near(near(3.0), |_| true));
        assert!(!grid.any_near(near(1.0), |_| true));
        assert!(!grid.any_near(near(50.0), |_| true));
        let empty = SegmentGrid::new(Vec::new(), 0.5);
        assert_eq!(empty.cast(Vec2::default(), Vec2::new(1.0, 0.0), f64::INFINITY), None);
        assert!(!empty.any_near(near(0.0), |_| true));
    }
}
