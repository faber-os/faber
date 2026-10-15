use super::*;
use crate::sim::world::{BaseSpec, DistanceCone, Footprint, Rng, STEP_S, ScanSpec, Twist, WorldSim, scan};
use faber_capabilities::Mount;
use std::collections::VecDeque;
use std::f64::consts::{FRAC_PI_2, TAU};

fn all() -> Vec<Prebuilt> {
    FILES.iter().map(|(name, _)| get(name).unwrap()).collect()
}

#[test]
fn every_world_parses_and_is_named_after_its_entry() {
    for (name, text) in FILES {
        let w = parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(w.name, *name);
        assert!(!w.title.is_empty() && w.description.len() > 20, "{name} needs a title and a description");
    }
    assert_eq!(
        list().iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
        ["empty", "apartment", "office", "warehouse", "maze", "obstacle-course"]
    );
    assert!(get("moon").is_none());
}

#[test]
fn every_world_is_closed_unless_it_says_it_is_open() {
    for w in all() {
        if w.geometry.open {
            continue;
        }
        let world = w.world();
        let o = w.start.position();
        for i in 0..3600 {
            let dir = Vec2::from_angle(f64::from(i) * TAU / 3600.0);
            assert!(
                world.cast(o, dir, f64::INFINITY, 0.0).is_some(),
                "{}: a ray from the start at {:.1}° escapes",
                w.name,
                f64::from(i) / 10.0
            );
        }
    }
}

#[test]
fn every_start_is_free_for_sennep() {
    for w in all() {
        assert!(
            !w.world().collides(w.start, Footprint::SENNEP, 0.0),
            "{}: SENNEP's 0.8 × 0.65 m footprint overlaps something at the start {:?}",
            w.name,
            w.start
        );
    }
}

#[test]
fn movers_never_walk_through_anything() {
    for w in all() {
        let world = w.world();
        for m in &w.geometry.movers {
            // A square around the person, slightly bigger than its circle: conservative.
            let around = Footprint { length: 2.0 * m.radius, width: 2.0 * m.radius };
            for k in 0..2000 {
                let p = m.position_at(f64::from(k) * 0.05);
                assert!(
                    !world.collides_static(Pose::new(p.x, p.y, 0.0), around),
                    "{}: the mover hits something at {p:?}",
                    w.name
                );
            }
        }
    }
}

/// Searches where SENNEP can drive from the start, breadth first over 0.1 m cells, facing the
/// start's way or turned 90° (turning in place where both fit), until `done(position, distance)`
/// says so. Returns whether it did.
fn search(w: &Prebuilt, mut done: impl FnMut(Vec2, f64) -> bool) -> bool {
    const CELL: f64 = 0.1;
    let world = w.world();
    let b = w.geometry.bounds;
    let (nx, ny) = ((b.size().x / CELL) as i64 + 1, (b.size().y / CELL) as i64 + 1);
    let pose = |x: i64, y: i64, turned: usize| {
        Pose::new(
            b.min.x + x as f64 * CELL,
            b.min.y + y as f64 * CELL,
            w.start.yaw + turned as f64 * FRAC_PI_2,
        )
    };
    let free = |p: Pose| !world.collides_static(p, Footprint::SENNEP);
    let sx = ((w.start.x - b.min.x) / CELL).round() as i64;
    let sy = ((w.start.y - b.min.y) / CELL).round() as i64;
    assert!(free(pose(sx, sy, 0)), "{}: the start cell is blocked", w.name);
    let mut seen = std::collections::HashSet::from([(sx, sy, 0usize)]);
    let mut queue = VecDeque::from([(sx, sy, 0usize, 0u32)]);
    while let Some((x, y, t, d)) = queue.pop_front() {
        if done(pose(x, y, t).position(), f64::from(d) * CELL) {
            return true;
        }
        let next = [(x + 1, y, t), (x - 1, y, t), (x, y + 1, t), (x, y - 1, t), (x, y, 1 - t)];
        for (nx_, ny_, nt) in next {
            if nx_ < 0 || ny_ < 0 || nx_ >= nx || ny_ >= ny || seen.contains(&(nx_, ny_, nt)) {
                continue;
            }
            if free(pose(nx_, ny_, nt)) {
                seen.insert((nx_, ny_, nt));
                // Turning in place costs no distance: 0-1 breadth-first search.
                if nt == t {
                    queue.push_back((nx_, ny_, nt, d + 1));
                } else {
                    queue.push_front((nx_, ny_, nt, d));
                }
            }
        }
    }
    false
}

#[test]
fn sennep_can_drive_more_than_3_m_from_every_start() {
    for w in all() {
        let mut longest = 0.0_f64;
        let far = search(&w, |_, d| {
            longest = longest.max(d);
            d > 3.0
        });
        assert!(far, "{}: SENNEP can only get {longest:.1} m from the start", w.name);
    }
}

#[test]
fn sennep_can_reach_every_room_of_the_apartment_through_its_doorways() {
    let w = get("apartment").unwrap();
    for (room, at) in [
        ("hallway", Vec2::new(3.0, 3.6)),
        ("bedroom", Vec2::new(2.4, 2.6)),
        ("bathroom", Vec2::new(4.6, 1.6)),
        ("kitchen", Vec2::new(1.8, 5.3)),
        ("study", Vec2::new(4.8, 5.0)),
    ] {
        assert!(search(&w, |p, _| (p - at).length() < 0.06), "SENNEP can't get into the {room}");
    }
}

#[test]
fn worlds_have_their_documented_sizes() {
    let size = |name: &str| get(name).unwrap().geometry.bounds.size();
    assert_eq!(size("empty"), Vec2::new(10.0, 10.0));
    let a = size("apartment");
    assert!((60.0..=80.0).contains(&(a.x * a.y)), "apartment is about 70 m²");
    assert_eq!(size("warehouse"), Vec2::new(30.0, 20.0));
}

#[test]
fn the_obstacle_course_has_a_walking_person() {
    let w = get("obstacle-course").unwrap();
    let m = &w.geometry.movers[0];
    assert_eq!(m.label.as_deref(), Some("person"));
    assert!((m.radius - 0.25).abs() < 0.05 && (m.speed - 1.0).abs() < 0.3);
    assert_ne!(m.position_at(0.0), m.position_at(1.0));
}

#[test]
fn table_legs_are_thin_and_their_tops_are_drawn_only() {
    let w = get("apartment").unwrap();
    let legs: Vec<_> =
        w.geometry.shapes.iter().filter(|s| s.label.as_deref() == Some("dining table")).collect();
    assert_eq!(legs.len(), 4);
    for l in legs {
        let side = (l.points[1] - l.points[0]).length();
        assert!((side - 0.04).abs() < 1e-9);
    }
    assert!(w.geometry.overhead.iter().any(|s| s.label.as_deref() == Some("dining table")));
}

#[test]
fn parse_rejects_mistakes_with_a_reason() {
    let head = "name = \"t\"\ntitle = \"T\"\ndescription = \"d\"\nstart = { x = 1.0, y = 1.0 }\n\
                bounds = { min = [0.0, 0.0], max = [4.0, 4.0] }\n";
    assert!(matches!(parse("name = 1"), Err(WorldError::Syntax(_))));
    assert!(matches!(parse(&format!("{head}colour = \"red\"")), Err(WorldError::Syntax(_))), "unknown keys");
    let door = format!("{head}[[wall]]\nfrom = [0.0, 2.0]\nto = [4.0, 2.0]\ndoors = [[3.5, 0.9]]\n");
    assert!(matches!(parse(&door), Err(WorldError::Invalid(m)) if m.contains("doorway")));
    let outside = format!("{head}[[box]]\nat = [5.0, 5.0]\nsize = [1.0, 1.0]\n");
    assert!(matches!(parse(&outside), Err(WorldError::Invalid(m)) if m.contains("outside")));
}

#[test]
fn walls_with_doorways_leave_gaps() {
    let text = "name = \"t\"\ntitle = \"T\"\ndescription = \"d\"\nstart = { x = 1.0, y = 1.0 }\n\
                bounds = { min = [0.0, 0.0], max = [6.0, 4.0] }\n\
                [[wall]]\nfrom = [0.0, 2.0]\nto = [6.0, 2.0]\ndoors = [[4.0, 0.9], [1.0, 0.8]]\n";
    let w = parse(text).unwrap();
    let want = [(0.0, 1.0), (1.8, 4.0), (4.9, 6.0)];
    assert_eq!(w.geometry.walls.len(), want.len());
    for (s, (a, b)) in w.geometry.walls.iter().zip(want) {
        assert!((s.a.x - a).abs() < 1e-9 && (s.b.x - b).abs() < 1e-9, "{s:?}");
        assert!(s.a.y == 2.0 && s.b.y == 2.0);
    }
}

/// Time for one 10 Hz tick of a typical machine: one LD19 revolution, four distance cones and
/// five 50 Hz steps, in the busiest prebuilt world. The bound is generous (the contract is "well
/// under 1 ms on a Pi"); run with `--release --nocapture` to see the numbers.
#[test]
fn a_sensor_tick_is_fast() {
    let cone = DistanceCone::new(0.02, 4.0, 25f64.to_radians());
    let cones: Vec<Mount> = (0..4)
        .map(|i| Mount { x: 0.4, yaw: (f64::from(i) * FRAC_PI_2) as f32, ..Mount::default() })
        .collect();
    let lidar = Mount { x: 0.2, z: 0.3, ..Mount::default() };
    let mut worst = 0.0_f64;
    for w in all() {
        let mut sim = WorldSim::new(w.world(), BaseSpec::new(Footprint::SENNEP, true, 1.0), w.start, 1);
        let ticks = 100;
        let t0 = std::time::Instant::now();
        for i in 0..ticks {
            sim.step(5.0 * STEP_S, Twist { omega: 0.5, ..Twist::ZERO });
            let r = sim.scan(&lidar, &ScanSpec::LD19, i);
            assert_eq!(r.len(), 450);
            for m in &cones {
                sim.distance(m, &cone);
            }
        }
        let per_tick_ms = t0.elapsed().as_secs_f64() * 1e3 / f64::from(ticks as u32);
        println!("{:16} {per_tick_ms:.3} ms per tick", w.name);
        worst = worst.max(per_tick_ms);
    }
    let bound = if cfg!(debug_assertions) { 40.0 } else { 1.0 };
    assert!(worst < bound, "slowest world takes {worst:.3} ms per tick (bound {bound} ms)");
}

#[test]
fn a_scan_in_the_apartment_sees_the_rooms_walls() {
    let w = get("apartment").unwrap();
    let r = scan(&w.world(), 0.0, w.start, &Mount::default(), &ScanSpec::LD19, &mut Rng::new(3), 0);
    let hits = r.ranges.iter().filter(|x| x.is_finite()).count();
    assert!(hits > 400, "a closed apartment returns nearly every ray: {hits} of 450");
}

#[test]
fn the_example_in_the_docs_is_a_valid_world() {
    let doc = include_str!("../../../../docs/simulation.md");
    let start =
        doc.find("```toml\nname = \"garage\"").expect("docs/simulation.md has the garage example") + 8;
    let end = start + doc[start..].find("```").unwrap();
    let w = parse(&doc[start..end]).unwrap();
    assert!(!w.world().collides(w.start, Footprint::SENNEP, 0.0));
    assert!(search(&w, |_, d| d > 3.0), "SENNEP can drive more than 3 m in the garage");
}
