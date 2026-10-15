//! Layer 3: the world engine (docs/simulation.md, sections 7 and 8). Every `simulate = "world"`
//! part lives in one shared [`WorldSim`]:
//! - one thread ticks the world at 50 Hz on the core's clock, while any world part runs, and
//!   stops (joined) when the last one goes;
//! - the world base (`planar-base@1`) moves the machine with its latest command, which the
//!   machine has already lease-checked and clamped, and publishes `commanded`, `measured` and
//!   `bumped`. The safe state is a zero command, so the machine ramps to a stop within its
//!   acceleration limit;
//! - world `laser-scan` and `distance` parts raycast from their mount at their rate;
//! - a world `battery` drains with the base's commanded speed: its idle current, plus its moving
//!   current in proportion to how close to top speed the base is asked to go;
//! - the machine's ground-truth pose is published as `world/pose` (`pose@1`).
//!
//! Without a world base the machine stands at the world's start pose, so sensors alone still see
//! the room. [`WorldEngine`] is the seam the machine drives; [`SimWorld`] implements it.

use crate::clock::SharedClock;
use crate::connectors::generic;
use crate::names::{PartName, SignalAddress};
use crate::router::Router;
use crate::sim::world::{
    BaseSpec, DistanceCone, DistanceReading, DistanceStatus, Footprint, MoverState, Pose, ScanSpec, Twist,
    World, WorldSim,
};
use crate::sim::worlds;
use crate::simulation::{Outlet, PartSpec, SimulatedPart};
use faber_capabilities::{
    Capability, Command, Info, Mount, Reading, Twist2d, battery, distance, planar_base, pose,
};
use faber_connector::{Applied, CommandError, Health, Problem, Settings};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the world moves on: 50 Hz, the world's own step (`sim::world::STEP_S`).
const TICK: Duration = Duration::from_millis(20);

/// The longest stretch of time one tick integrates. If the core stalls (a debugger, a suspended
/// laptop), the world pauses with it instead of jumping ahead.
const MAX_STEP_S: f64 = 0.5;

/// Sensor noise seed. Fixed, so a reset repeats the same readings.
const SEED: u64 = 0x5EED_FAB3;

/// The part name of the world's own signals: `world/pose`.
pub const WORLD_PART: &str = "world";

/// `world/pose`: the machine's ground-truth pose in the world.
pub fn pose_address() -> SignalAddress {
    SignalAddress::new(PartName::parse(WORLD_PART).expect("a valid part name"), "pose")
}

/// What the machine needs from a world engine.
pub trait WorldEngine: Send + Sync {
    /// Where the world's own signals (`world/pose`) go. The machine calls it once, at start.
    fn attach(&self, router: Arc<Router>);

    /// Starts a `simulate = "world"` part.
    fn start(&self, part: PartSpec) -> Box<dyn SimulatedPart>;

    /// Replaces the world and puts the machine at rest at `start`. Running world parts stay and
    /// sense the new world.
    fn load(&self, world: World, start: Pose);

    /// Puts the machine back at the start pose, at rest.
    fn reset(&self);

    /// The machine in the world now.
    fn state(&self) -> WorldState;
}

/// The machine in the world at one moment: what the `world` WebSocket topic sends.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldState {
    /// When the world last moved on (µs, core clock); 0 before it has ticked.
    pub t_us: u64,
    pub pose: Pose,
    /// Machine frame: `vx` forward, `vy` left, `omega` counter-clockwise.
    pub velocity: Twist,
    /// Something stopped the machine during its last step.
    pub bumped: bool,
    pub movers: Vec<MoverState>,
    /// The world base's footprint; `None` while there is no world base.
    pub footprint: Option<Footprint>,
}

/// The world engine: one [`WorldSim`] for every world part.
pub struct SimWorld {
    shared: Arc<Shared>,
}

struct Shared {
    engine: Mutex<Engine>,
    router: Mutex<Option<Arc<Router>>>,
    ticker: Mutex<Option<Ticker>>,
    /// Ticker threads alive right now: 1 while world parts run, else 0.
    threads: Arc<AtomicUsize>,
    /// `false` in tests that tick by hand.
    autotick: bool,
}

struct Ticker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Default for SimWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl SimWorld {
    /// An engine holding the `empty` world until the machine loads the configured one.
    pub fn new() -> SimWorld {
        Self::with_ticking(true)
    }

    fn with_ticking(autotick: bool) -> SimWorld {
        let empty = worlds::get("empty").expect("the empty world is built in");
        SimWorld {
            shared: Arc::new(Shared {
                engine: Mutex::new(Engine::new(empty.world(), empty.start)),
                router: Mutex::new(None),
                ticker: Mutex::new(None),
                threads: Arc::new(AtomicUsize::new(0)),
                autotick,
            }),
        }
    }

    /// Ticker threads alive now: 1 while any world part runs, 0 otherwise.
    pub fn live_threads(&self) -> usize {
        self.shared.threads.load(Ordering::SeqCst)
    }
}

impl WorldEngine for SimWorld {
    fn attach(&self, router: Arc<Router>) {
        *self.shared.router.lock().expect("world router lock") = Some(router);
    }

    fn start(&self, part: PartSpec) -> Box<dyn SimulatedPart> {
        let mut ids = Vec::new();
        let mut problems = Vec::new();
        let mut base_refused = false;
        {
            let mut e = self.shared.engine.lock().expect("world lock");
            for p in &part.provides {
                match (p.capability, &p.info) {
                    (Capability::PlanarBase, Some(Info::PlanarBase(info))) => {
                        if e.base.is_some() {
                            base_refused = true;
                            problems.push("The world already has a base: the machine itself.".to_string());
                            continue;
                        }
                        let id = e.next_id();
                        let (length, width) = generic::footprint(&part.settings);
                        e.base = Some(Base {
                            id,
                            outlet: Arc::clone(&part.outlet),
                            name: p.name.clone(),
                            info: info.clone(),
                            footprint: Footprint { length, width },
                            commanded: Twist2d::ZERO,
                            every_us: every_us(&part.settings, 20.0),
                            due_us: 0,
                        });
                        e.rebuild();
                        ids.push(id);
                    }
                    (Capability::LaserScan, info) => {
                        let mut spec = ScanSpec::LD19;
                        if let Some(Info::LaserScan(i)) = info {
                            spec.range_min = f64::from(i.range_min_m);
                            spec.range_max = f64::from(i.range_max_m);
                        }
                        let every = every_us(&part.settings, 1.0 / spec.scan_time);
                        spec.scan_time = every as f64 / 1e6;
                        let id = e.next_id();
                        e.sensors.push(Sensor::new(id, &part, p.name.clone(), SensorKind::Scan(spec), every));
                        ids.push(id);
                    }
                    (Capability::Distance, Some(Info::Distance(i))) => {
                        let cone = DistanceCone::new(
                            f64::from(i.min_m),
                            f64::from(i.max_m),
                            f64::from(i.field_of_view_rad),
                        );
                        let every = every_us(&part.settings, 10.0);
                        let id = e.next_id();
                        let kind = SensorKind::Distance(i.clone(), cone);
                        e.sensors.push(Sensor::new(id, &part, p.name.clone(), kind, every));
                        ids.push(id);
                    }
                    (Capability::Battery, Some(Info::Battery(i))) => {
                        let every = every_us(&part.settings, 2.0);
                        let id = e.next_id();
                        let kind = SensorKind::Battery(WorldBattery::new(
                            i.clone(),
                            generic::battery_drain(&part.settings),
                        ));
                        e.sensors.push(Sensor::new(id, &part, p.name.clone(), kind, every));
                        ids.push(id);
                    }
                    (c, _) => problems.push(format!("The world can't simulate {} yet.", c.id())),
                }
            }
        }
        part.outlet.set_health(if problems.is_empty() {
            Health::Running
        } else {
            Health::Degraded(Problem::new(
                problems.join(" "),
                "Use simulate = \"manual\" for this part, or remove the other world base.",
            ))
        });
        if !ids.is_empty() {
            self.shared.ensure_ticking(&self.shared, &part.clock);
        }
        Box::new(WorldPart {
            shared: Arc::clone(&self.shared),
            ids,
            base_refused,
            outlet: part.outlet,
            clock: part.clock,
        })
    }

    fn load(&self, world: World, start: Pose) {
        let mut e = self.shared.engine.lock().expect("world lock");
        e.world = world;
        e.start = start;
        e.rebuild();
        for s in &mut e.sensors {
            s.due_us = 0;
        }
        if e.sim.collides(start) {
            tracing::warn!(
                target: "core::world",
                "the machine's start pose ({:.2}, {:.2}) overlaps something in the world, so it can't move; pick another start",
                start.x,
                start.y
            );
        }
    }

    fn reset(&self) {
        let mut e = self.shared.engine.lock().expect("world lock");
        e.sim.reset();
        e.bumped = false;
    }

    fn state(&self) -> WorldState {
        let e = self.shared.engine.lock().expect("world lock");
        WorldState {
            t_us: e.last_us.unwrap_or(0),
            pose: e.sim.pose(),
            velocity: e.sim.velocity(),
            bumped: e.bumped,
            movers: e.sim.movers(),
            footprint: e.base.as_ref().map(|b| b.footprint),
        }
    }
}

impl Shared {
    /// Starts the ticker thread unless it runs, and declares `world/pose`.
    fn ensure_ticking(&self, this: &Arc<Shared>, clock: &SharedClock) {
        if !self.autotick {
            return;
        }
        let mut ticker = self.ticker.lock().expect("ticker lock");
        if ticker.is_some() {
            return;
        }
        if let Some(router) = self.router.lock().expect("world router lock").as_ref() {
            router.declare(pose_address(), Capability::Pose);
        }
        self.engine.lock().expect("world lock").last_us = None;
        let stop = Arc::new(AtomicBool::new(false));
        let threads = Arc::clone(&self.threads);
        threads.fetch_add(1, Ordering::SeqCst);
        let (weak, clock, flag) = (Arc::downgrade(this), Arc::clone(clock), Arc::clone(&stop));
        let handle = std::thread::Builder::new()
            .name("world".into())
            .spawn(move || {
                run_ticker(weak, clock, flag);
                threads.fetch_sub(1, Ordering::SeqCst);
            })
            .expect("spawn the world ticker");
        *ticker = Some(Ticker { stop, handle });
    }

    /// Removes the parts `ids`; stops the ticker (and removes `world/pose`) if none are left.
    fn remove(&self, ids: &[u64]) {
        let mut ticker = self.ticker.lock().expect("ticker lock");
        let empty = {
            let mut e = self.engine.lock().expect("world lock");
            e.remove(ids);
            e.is_empty()
        };
        if !empty {
            return;
        }
        if let Some(t) = ticker.take() {
            t.stop.store(true, Ordering::SeqCst);
            let _ = t.handle.join();
            if let Some(router) = self.router.lock().expect("world router lock").as_ref() {
                router.remove(&pose_address());
            }
        }
    }

    /// Moves the world on to `now` and publishes what is due. Publishing happens under the
    /// world lock, so a part that is being removed never publishes after it is gone.
    fn tick(&self, now: u64) {
        let mut e = self.engine.lock().expect("world lock");
        let tick = e.tick(now);
        for (outlet, name, reading) in tick.readings {
            outlet.publish(&name, reading);
        }
        if let Some(router) = self.router.lock().expect("world router lock").as_ref()
            && let Err(err) = router.publish(&pose_address(), tick.pose)
        {
            tracing::error!(target: "core::world", "world/pose could not be published: {err:?}");
        }
    }
}

fn run_ticker(shared: Weak<Shared>, clock: SharedClock, stop: Arc<AtomicBool>) {
    let mut next = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let Some(s) = shared.upgrade() else { return };
        s.tick(clock.now_us());
        drop(s);
        next += TICK;
        let now = Instant::now();
        if next < now {
            next = now; // fell behind: don't try to catch up with a burst of ticks
        }
        while !stop.load(Ordering::SeqCst) {
            let left = next.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
    }
}

/// The publishing period (µs) from a part's `rate-hz` setting, or `default_hz`.
fn every_us(settings: &Settings, default_hz: f64) -> u64 {
    let hz = settings
        .get("rate-hz")
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .filter(|r| r.is_finite() && *r > 0.0)
        .unwrap_or(default_hz);
    (1e6 / hz).round().max(1.0) as u64
}

/// `true` (and the next due time set) if a reading due at `due_us` is due at `now`.
fn due(due_us: &mut u64, every_us: u64, now: u64) -> bool {
    if now < *due_us {
        return false;
    }
    *due_us += every_us;
    if *due_us <= now {
        *due_us = now + every_us; // fell behind: skip, don't burst
    }
    true
}

// ---- the engine: pure state, ticked by the thread (or by hand in tests) ----------------------

struct Base {
    id: u64,
    outlet: Arc<dyn Outlet>,
    name: String,
    info: planar_base::Info,
    footprint: Footprint,
    commanded: Twist2d,
    every_us: u64,
    due_us: u64,
}

enum SensorKind {
    Scan(ScanSpec),
    Distance(distance::Info, DistanceCone),
    Battery(WorldBattery),
}

/// A battery in the world: it drains as the machine drives.
struct WorldBattery {
    info: battery::Info,
    drain: generic::Drain,
    /// 0-1.
    charge: f32,
    /// What it draws now (A).
    amps: f32,
    /// The level reported last, for its hysteresis.
    level: Option<battery::Level>,
}

impl WorldBattery {
    fn new(info: battery::Info, drain: generic::Drain) -> WorldBattery {
        WorldBattery { info, charge: drain.start_charge, drain, amps: drain.idle_a, level: None }
    }

    /// Draws for `dt_s` seconds with the base asked to go at `load` (0-1) of its top speed.
    fn run(&mut self, load: f32, dt_s: f64) {
        self.amps = self.drain.idle_a + self.drain.moving_a * load.clamp(0.0, 1.0);
        let capacity_as = f64::from(self.info.capacity_ah.unwrap_or(f32::INFINITY)) * 3600.0;
        let used = f64::from(self.amps) * dt_s / capacity_as;
        self.charge = (self.charge - used as f32).max(0.0);
    }

    /// Its resting voltage at its charge (no sag under load), the current as discharge, and the
    /// level with the capability's hysteresis.
    fn reading(&mut self, t_us: u64) -> battery::Reading {
        let volts = self.info.volts_at(self.charge);
        let level = self.info.level(volts, self.level);
        self.level = Some(level);
        battery::Reading {
            amps: Some(-self.amps),
            charge: Some(self.charge),
            ..battery::Reading::volts(t_us, volts, level)
        }
    }
}

/// How close to its top speed a base is asked to go, 0-1: the largest share on any axis.
fn speed_share(commanded: Twist2d, max: Twist2d) -> f32 {
    let share = |v: f32, m: f32| if m > 0.0 { (v / m).abs() } else { 0.0 };
    share(commanded.vx, max.vx)
        .max(share(commanded.vy, max.vy))
        .max(share(commanded.omega, max.omega))
        .min(1.0)
}

struct Sensor {
    id: u64,
    outlet: Arc<dyn Outlet>,
    name: String,
    mount: Mount,
    kind: SensorKind,
    every_us: u64,
    due_us: u64,
}

impl Sensor {
    fn new(id: u64, part: &PartSpec, name: String, kind: SensorKind, every_us: u64) -> Sensor {
        Sensor { id, outlet: Arc::clone(&part.outlet), name, mount: part.mount, kind, every_us, due_us: 0 }
    }
}

struct Engine {
    world: World,
    start: Pose,
    sim: WorldSim,
    base: Option<Base>,
    sensors: Vec<Sensor>,
    next_id: u64,
    /// When the world last moved on; `None` until the ticker's first tick.
    last_us: Option<u64>,
    /// Whether the last integrated step bumped. Kept across ticks that integrate nothing, so it
    /// doesn't flicker when a tick comes a little early.
    bumped: bool,
}

/// What one tick publishes.
struct Tick {
    readings: Vec<(Arc<dyn Outlet>, String, Reading)>,
    pose: Reading,
}

impl Engine {
    fn new(world: World, start: Pose) -> Engine {
        let sim = WorldSim::new(world.clone(), no_base(), start, SEED);
        Engine {
            world,
            start,
            sim,
            base: None,
            sensors: Vec::new(),
            next_id: 1,
            last_us: None,
            bumped: false,
        }
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn is_empty(&self) -> bool {
        self.base.is_none() && self.sensors.is_empty()
    }

    fn base_spec(&self) -> BaseSpec {
        let Some(b) = &self.base else { return no_base() };
        BaseSpec {
            footprint: b.footprint,
            holonomic: b.info.holonomic,
            max_accel: f64::from(b.info.max_acceleration.vx),
            max_angular_accel: f64::from(b.info.max_acceleration.omega),
        }
    }

    /// A fresh simulation of the current world and base: the machine at rest at the start.
    fn rebuild(&mut self) {
        self.sim = WorldSim::new(self.world.clone(), self.base_spec(), self.start, SEED);
        self.bumped = false;
    }

    fn remove(&mut self, ids: &[u64]) {
        self.sensors.retain(|s| !ids.contains(&s.id));
        if self.base.as_ref().is_some_and(|b| ids.contains(&b.id)) {
            self.base = None;
            self.rebuild();
        }
    }

    fn base_reading(&self, t_us: u64) -> Option<Reading> {
        let b = self.base.as_ref()?;
        let v = self.sim.velocity();
        Some(Reading::PlanarBase(planar_base::Reading {
            t_us,
            commanded: b.commanded,
            measured: Some(Twist2d::new(v.vx as f32, v.vy as f32, v.omega as f32)),
            bumped: self.bumped,
        }))
    }

    fn tick(&mut self, now: u64) -> Tick {
        let dt = self.last_us.map_or(0.0, |l| now.saturating_sub(l) as f64 / 1e6).min(MAX_STEP_S);
        self.last_us = Some(now);
        let command = self.base.as_ref().map_or(Twist::ZERO, |b| Twist {
            vx: f64::from(b.commanded.vx),
            vy: f64::from(b.commanded.vy),
            omega: f64::from(b.commanded.omega),
        });
        let before = self.sim.time_s();
        let motion = self.sim.step(dt, command);
        if self.sim.time_s() != before {
            self.bumped = motion.bumped;
        }
        let mut readings = Vec::new();
        let load = self.base.as_ref().map_or(0.0, |b| speed_share(b.commanded, b.info.max_velocity));
        for s in &mut self.sensors {
            if let SensorKind::Battery(b) = &mut s.kind {
                b.run(load, dt);
            }
        }
        let base_due = self.base.as_mut().is_some_and(|b| due(&mut b.due_us, b.every_us, now));
        if base_due && let (Some(b), Some(r)) = (self.base.as_ref(), self.base_reading(now)) {
            readings.push((Arc::clone(&b.outlet), b.name.clone(), r));
        }
        let Engine { sim, sensors, .. } = self;
        for s in sensors.iter_mut() {
            if !due(&mut s.due_us, s.every_us, now) {
                continue;
            }
            let reading = match &mut s.kind {
                SensorKind::Scan(spec) => Reading::LaserScan(sim.scan(&s.mount, spec, now)),
                SensorKind::Distance(info, cone) => {
                    Reading::Distance(distance_reading(now, info, sim.distance(&s.mount, cone)))
                }
                SensorKind::Battery(b) => Reading::Battery(b.reading(now)),
            };
            readings.push((Arc::clone(&s.outlet), s.name.clone(), reading));
        }
        let p = self.sim.pose();
        let pose =
            Reading::Pose(pose::Reading::planar(now, WORLD_PART, p.x as f32, p.y as f32, p.yaw as f32));
        Tick { readings, pose }
    }
}

/// The machine when no world base exists: it stands where it starts.
fn no_base() -> BaseSpec {
    let (length, width) = generic::DEFAULT_FOOTPRINT;
    BaseSpec::new(Footprint { length, width }, true, 1.0)
}

fn distance_reading(t_us: u64, info: &distance::Info, d: DistanceReading) -> distance::Reading {
    let (meters, status) = match d.status {
        DistanceStatus::Ok => (d.meters as f32, distance::Status::Ok),
        DistanceStatus::TooClose => (info.min_m, distance::Status::TooClose),
        DistanceStatus::TooFar => (info.max_m, distance::Status::TooFar),
        DistanceStatus::NoTarget => (info.max_m, distance::Status::NoTarget),
    };
    distance::Reading { t_us, meters, status }
}

// ---- a running world part ---------------------------------------------------------------------

struct WorldPart {
    shared: Arc<Shared>,
    /// Its base and sensors in the engine.
    ids: Vec<u64>,
    /// It is a base, but the world already had one.
    base_refused: bool,
    outlet: Arc<dyn Outlet>,
    clock: SharedClock,
}

impl SimulatedPart for WorldPart {
    fn command(&mut self, command: Command) -> Result<Applied, CommandError> {
        let Command::PlanarBase(planar_base::Command::SetVelocity(twist)) = command else {
            return Err(CommandError::Unsupported);
        };
        let now = self.clock.now_us();
        let mut e = self.shared.engine.lock().expect("world lock");
        let Some(b) = e.base.as_mut().filter(|b| self.ids.contains(&b.id)) else {
            return Err(if self.base_refused {
                CommandError::NotReady(Problem::new(
                    "This base isn't in the world: another world base is the machine.",
                    "Remove the other world base, or simulate this one with simulate = \"manual\".",
                ))
            } else {
                CommandError::Unsupported
            });
        };
        b.commanded = twist;
        let name = b.name.clone();
        let reading = e.base_reading(now).expect("the base is there");
        self.outlet.publish(&name, reading);
        Ok(Applied { command })
    }
}

impl Drop for WorldPart {
    fn drop(&mut self) {
        self.shared.remove(&self.ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, ManualClock, MonotonicClock};
    use crate::router::Subscription;
    use crate::sim::world::{Bounds, Geometry, Segment, Vec2};
    use faber_capabilities::laser_scan;

    #[derive(Default)]
    struct Recorder {
        published: Mutex<Vec<(String, Reading)>>,
        health: Mutex<Vec<Health>>,
    }

    impl Outlet for Recorder {
        fn publish(&self, name: &str, reading: Reading) {
            self.published.lock().unwrap().push((name.into(), reading));
        }
        fn set_health(&self, health: Health) {
            self.health.lock().unwrap().push(health);
        }
    }

    impl Recorder {
        fn readings(&self) -> Vec<Reading> {
            self.published.lock().unwrap().iter().map(|(_, r)| r.clone()).collect()
        }

        fn bases(&self) -> Vec<planar_base::Reading> {
            self.readings()
                .into_iter()
                .filter_map(|r| match r {
                    Reading::PlanarBase(b) => Some(b),
                    _ => None,
                })
                .collect()
        }

        fn last_base(&self) -> planar_base::Reading {
            self.bases().pop().expect("a base reading")
        }

        fn last_health(&self) -> Option<Health> {
            self.health.lock().unwrap().last().cloned()
        }
    }

    /// A 6 × 4 m room, x from 0 to 6, y from 0 to 4.
    fn room() -> World {
        let c = [Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(6.0, 4.0), Vec2::new(0.0, 4.0)];
        World::new(Geometry {
            bounds: Bounds { min: c[0], max: c[2] },
            walls: (0..4).map(|i| Segment::new(c[i], c[(i + 1) % 4])).collect(),
            ..Geometry::default()
        })
    }

    fn base_info() -> Info {
        Info::PlanarBase(planar_base::Info {
            holonomic: true,
            max_velocity: Twist2d::new(1.0, 1.0, 2.0),
            max_acceleration: Twist2d::new(1.0, 1.0, 2.0),
            built_from: vec![],
        })
    }

    fn distance_info() -> distance::Info {
        distance::Info { min_m: 0.02, max_m: 4.0, field_of_view_rad: 0.0, kind: distance::Kind::LaserTof }
    }

    fn lidar_info() -> Info {
        Info::LaserScan(laser_scan::Info { range_min_m: 0.02, range_max_m: 12.0 })
    }

    fn spec(
        info: Info,
        mount: Mount,
        settings: &str,
        outlet: &Arc<Recorder>,
        clock: SharedClock,
    ) -> PartSpec {
        PartSpec {
            name: PartName::parse("part").unwrap(),
            connector: "x".into(),
            provides: vec![faber_connector::Provide::with_info(info)],
            mount,
            settings: toml::from_str(settings).unwrap(),
            outlet: Arc::clone(outlet) as Arc<dyn Outlet>,
            clock,
        }
    }

    /// A world that only moves when the test says so, in the 6 × 4 room, with a router attached.
    fn paused(start: Pose) -> (SimWorld, Arc<Router>, Arc<ManualClock>) {
        let w = SimWorld::with_ticking(false);
        let router = Arc::new(Router::new());
        router.declare(pose_address(), Capability::Pose);
        w.attach(Arc::clone(&router));
        w.load(room(), start);
        (w, router, Arc::new(ManualClock::new(0)))
    }

    /// Ticks every 20 ms from the clock's time for `secs`, calling `each` after every tick.
    fn run(w: &SimWorld, clock: &ManualClock, secs: f64, mut each: impl FnMut(&WorldState)) {
        for _ in 0..(secs / 0.02).round() as u32 {
            clock.advance(20_000);
            w.shared.tick(clock.now_us());
            each(&w.state());
        }
    }

    fn set_velocity(vx: f32, vy: f32, omega: f32) -> Command {
        Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(vx, vy, omega)))
    }

    fn battery_info() -> Info {
        Info::Battery(battery::Info {
            chemistry: battery::Chemistry::LiIon,
            cells_in_series: 3,
            capacity_ah: Some(1.0),
        })
    }

    fn batteries(out: &Recorder) -> Vec<battery::Reading> {
        out.readings()
            .into_iter()
            .filter_map(|r| match r {
                Reading::Battery(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_world_battery_drains_faster_the_faster_the_base_is_asked_to_go() {
        let (w, _, clock) = paused(Pose::new(3.0, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let settings = "idle-current-a = 0.36\nmoving-current-a = 3.6\nrate-hz = 1";
        let _battery = w.start(spec(battery_info(), Mount::default(), settings, &out, clock.clone()));
        assert_eq!(out.last_health(), Some(Health::Running));
        let base_out = Arc::new(Recorder::default());
        let mut base = w.start(spec(base_info(), Mount::default(), "", &base_out, clock.clone()));
        w.shared.tick(0);
        run(&w, &clock, 10.0, |_| {});
        let idle = batteries(&out);
        let first = idle.first().unwrap();
        assert_eq!(first.level, battery::Level::Ok);
        assert_eq!((first.charge, first.amps), (Some(1.0), Some(-0.36)), "full, drawing its idle current");
        assert!((first.volts - 12.6).abs() < 1e-4);
        // 0.36 A for 10 s from 1 Ah (3600 As) is 0.1 %.
        let after_idle = idle.last().unwrap().charge.unwrap();
        assert!((after_idle - 0.999).abs() < 1e-4, "{after_idle}");
        // Half speed sideways: 0.36 + 3.6 × 0.5 = 2.16 A, six times the idle drain.
        base.command(set_velocity(0.0, 0.5, 0.0)).unwrap();
        run(&w, &clock, 10.0, |_| {});
        let last = batteries(&out).pop().unwrap();
        assert!((last.amps.unwrap() + 2.16).abs() < 1e-5, "{last:?}");
        assert!((after_idle - last.charge.unwrap() - 0.006).abs() < 2e-4, "{last:?}");
        assert!(last.volts < 12.6);
    }

    #[test]
    fn a_world_battery_runs_down_to_low_and_critical_and_stops_at_empty() {
        let (w, _, clock) = paused(Pose::new(3.0, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let settings = "idle-current-a = 36\nmoving-current-a = 0\nstart-charge = 0.2\nrate-hz = 10";
        let _battery = w.start(spec(battery_info(), Mount::default(), settings, &out, clock.clone()));
        w.shared.tick(0);
        // 36 A from 1 Ah is 1 % a second: 20 % lasts 20 s.
        run(&w, &clock, 30.0, |_| {});
        let levels: Vec<battery::Level> = batteries(&out).iter().map(|b| b.level).collect();
        let mut changes = levels.clone();
        changes.dedup();
        assert_eq!(changes, [battery::Level::Ok, battery::Level::Low, battery::Level::Critical]);
        let last = batteries(&out).pop().unwrap();
        assert_eq!(last.charge, Some(0.0), "an empty battery stays empty");
        assert!((last.volts - 9.0).abs() < 1e-4);
    }

    #[test]
    fn a_world_base_drives_the_machine_and_reports_commanded_and_measured() {
        let (w, _, clock) = paused(Pose::new(1.0, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let mut base =
            w.start(spec(base_info(), Mount::default(), "length = 0.8\nwidth = 0.65", &out, clock.clone()));
        assert_eq!(out.last_health(), Some(Health::Running));
        assert_eq!(w.state().footprint, Some(Footprint { length: 0.8, width: 0.65 }));
        w.shared.tick(0);
        base.command(set_velocity(0.5, 0.0, 0.0)).unwrap();
        let r = out.last_base();
        assert_eq!((r.commanded.vx, r.measured.unwrap().vx), (0.5, 0.0), "published at once; it can't jump");
        run(&w, &clock, 0.2, |_| {});
        assert!((out.last_base().measured.unwrap().vx - 0.2).abs() < 1e-5, "1 m/s² for 0.2 s");
        run(&w, &clock, 0.8, |_| {});
        let s = w.state();
        assert_eq!(out.last_base().measured.unwrap().vx, 0.5);
        assert!(!s.bumped && s.velocity.vx == 0.5);
        // 25 steps ramping (0.02..0.5 m/s), then 25 cruising.
        let ramp: f64 = (1..=25).map(|k| 0.02 * f64::from(k) * 0.02).sum();
        assert!((s.pose.x - (1.0 + ramp + 25.0 * 0.5 * 0.02)).abs() < 1e-6, "{s:?}");
        assert_eq!(s.pose.y, 2.0);
    }

    #[test]
    fn the_safe_state_ramps_to_a_stop_within_the_acceleration_limit() {
        let (w, _, clock) = paused(Pose::new(1.0, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let mut base = w.start(spec(base_info(), Mount::default(), "", &out, clock.clone()));
        w.shared.tick(0);
        base.command(set_velocity(0.5, 0.0, 0.0)).unwrap();
        run(&w, &clock, 1.0, |_| {});
        base.command(Command::PlanarBase(planar_base::Command::STOP)).unwrap();
        assert_eq!(out.last_base().commanded, Twist2d::ZERO, "the zero command is reported at once");
        let mut speeds = Vec::new();
        run(&w, &clock, 0.6, |s| speeds.push(s.velocity.vx));
        assert!((speeds[0] - 0.48).abs() < 1e-9, "slows by 1 m/s² × 20 ms per step: {speeds:?}");
        assert!(speeds[23] > 0.0 && speeds[24] == 0.0, "stopped after 0.5 s: {speeds:?}");
        assert_eq!(out.last_base().measured, Some(Twist2d::ZERO));
    }

    #[test]
    fn driving_into_a_wall_bumps_and_never_passes_it() {
        let (w, _, clock) = paused(Pose::new(4.5, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let mut base =
            w.start(spec(base_info(), Mount::default(), "length = 0.8\nwidth = 0.65", &out, clock.clone()));
        w.shared.tick(0);
        base.command(set_velocity(0.5, 0.0, 0.0)).unwrap();
        let mut furthest: f64 = 0.0;
        run(&w, &clock, 4.0, |s| furthest = furthest.max(s.pose.x + 0.4));
        assert!(furthest <= 6.0, "the front edge never passes the east wall: {furthest}");
        assert!(furthest > 6.0 - 0.01, "and it got right up to it: {furthest}");
        let r = out.last_base();
        assert!(r.bumped, "{r:?}");
        assert_eq!((r.commanded.vx, r.measured.unwrap().vx), (0.5, 0.0), "pushing on, but stopped");
        assert!(w.state().bumped);
        base.command(set_velocity(-0.5, 0.0, 0.0)).unwrap();
        run(&w, &clock, 0.2, |_| {});
        assert!(!out.last_base().bumped, "backing off clears it");
    }

    #[test]
    fn sensors_cast_from_their_mount_at_their_rate() {
        let (w, _, clock) = paused(Pose::new(3.0, 2.0, 0.0));
        let base_out = Arc::new(Recorder::default());
        let _base = w.start(spec(base_info(), Mount::default(), "", &base_out, clock.clone()));
        let lidar = Arc::new(Recorder::default());
        let _l = w.start(spec(
            lidar_info(),
            Mount { x: 0.2, z: 0.3, ..Mount::default() },
            "",
            &lidar,
            clock.clone(),
        ));
        let front = Arc::new(Recorder::default());
        let _f = w.start(spec(
            Info::Distance(distance_info()),
            Mount { x: 0.4, ..Mount::default() },
            "rate-hz = 50",
            &front,
            clock.clone(),
        ));
        w.shared.tick(0);
        run(&w, &clock, 1.0, |_| {});
        let scans: Vec<_> = lidar
            .readings()
            .into_iter()
            .map(|r| match r {
                Reading::LaserScan(s) => s,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(scans.len(), 11, "the LD19 at 10 Hz over 1 s, both ends included");
        let s = &scans[0];
        assert_eq!(s.len(), 450);
        assert!((s.scan_time - 0.1).abs() < 1e-6);
        let ahead = (0..s.len()).min_by(|&a, &b| s.angle(a).abs().total_cmp(&s.angle(b).abs())).unwrap();
        assert!(
            (s.ranges[ahead] - 2.8).abs() < 0.05,
            "east wall 2.8 m ahead of the LiDAR: {}",
            s.ranges[ahead]
        );
        let ds: Vec<_> = front.readings();
        assert_eq!(ds.len(), 51, "every tick at 50 Hz");
        let Reading::Distance(d) = &ds[0] else { panic!() };
        assert_eq!(d.status, distance::Status::Ok);
        assert!((d.meters - 2.6).abs() < 0.03, "{d:?}");
        assert_eq!(base_out.bases().len(), 21, "a base at its default 20 Hz");
    }

    #[test]
    fn a_sensor_without_a_world_base_sees_from_the_start_pose() {
        let (w, router, clock) = paused(Pose::new(3.0, 2.0, std::f64::consts::PI));
        let out = Arc::new(Recorder::default());
        let _d = w.start(spec(Info::Distance(distance_info()), Mount::default(), "", &out, clock.clone()));
        assert_eq!(w.state().footprint, None);
        w.shared.tick(0);
        run(&w, &clock, 0.5, |_| {});
        let Some(Reading::Distance(d)) = out.readings().pop() else { panic!() };
        assert!((d.meters - 3.0).abs() < 0.03, "facing west, the west wall is 3 m away: {d:?}");
        assert_eq!(w.state().pose, Pose::new(3.0, 2.0, std::f64::consts::PI));
        let Some(Subscription::Latest(rx)) = router.subscribe(&pose_address()) else { panic!() };
        let Some(Reading::Pose(p)) = rx.borrow().as_deref().cloned() else { panic!("no world/pose") };
        assert_eq!((p.frame.as_str(), p.position.x, p.position.y), ("world", 3.0, 2.0));
    }

    #[test]
    fn distance_out_of_range_reports_the_edge_of_its_range() {
        let info = distance::Info { min_m: 0.5, max_m: 1.0, ..distance_info() };
        let far = DistanceReading { meters: f64::INFINITY, status: DistanceStatus::TooFar };
        assert_eq!(distance_reading(1, &info, far).meters, 1.0);
        let near = DistanceReading { meters: 0.5, status: DistanceStatus::TooClose };
        assert_eq!(distance_reading(1, &info, near).status, distance::Status::TooClose);
        let none = DistanceReading { meters: f64::INFINITY, status: DistanceStatus::NoTarget };
        assert!(distance_reading(1, &info, none).meters.is_finite(), "distance@1 meters are always finite");
    }

    #[test]
    fn loading_a_world_or_resetting_puts_the_machine_back_at_rest_at_the_start() {
        let (w, _, clock) = paused(Pose::new(1.0, 2.0, 0.0));
        let out = Arc::new(Recorder::default());
        let mut base = w.start(spec(base_info(), Mount::default(), "", &out, clock.clone()));
        w.shared.tick(0);
        base.command(set_velocity(0.5, 0.0, 0.0)).unwrap();
        run(&w, &clock, 0.5, |_| {});
        assert!(w.state().pose.x > 1.05);
        w.reset();
        let s = w.state();
        assert_eq!((s.pose, s.velocity), (Pose::new(1.0, 2.0, 0.0), Twist::ZERO));
        run(&w, &clock, 0.5, |_| {});
        let apartment = worlds::get("apartment").unwrap();
        w.load(apartment.world(), apartment.start);
        let s = w.state();
        assert_eq!((s.pose, s.velocity), (apartment.start, Twist::ZERO));
        run(&w, &clock, 0.2, |_| {});
        assert!(w.state().pose.x < apartment.start.x, "it keeps driving (west) in the new world");
    }

    #[test]
    fn what_the_world_cant_simulate_is_degraded_and_says_so() {
        let (w, _, clock) = paused(Pose::new(1.0, 2.0, 0.0));
        let first = Arc::new(Recorder::default());
        let _first = w.start(spec(base_info(), Mount::default(), "", &first, clock.clone()));
        let second = Arc::new(Recorder::default());
        let mut other = w.start(spec(base_info(), Mount::default(), "", &second, clock.clone()));
        let Some(Health::Degraded(p)) = second.last_health() else { panic!() };
        assert!(p.message.contains("already has a base"), "{p:?}");
        assert!(matches!(other.command(set_velocity(0.1, 0.0, 0.0)), Err(CommandError::NotReady(_))));
        let motor = Arc::new(Recorder::default());
        let info = Info::Motor(faber_capabilities::motor::Info {
            kind: faber_capabilities::motor::Kind::Dc,
            modes: faber_capabilities::motor::Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: Some(1.0),
        });
        let mut m = w.start(spec(info, Mount::default(), "", &motor, clock.clone()));
        let Some(Health::Degraded(p)) = motor.last_health() else { panic!() };
        assert!(p.message.contains("motor@1"), "{p:?}");
        let cmd = Command::Motor(faber_capabilities::motor::Command::SetDuty { duty: 0.5 });
        assert_eq!(m.command(cmd).unwrap_err(), CommandError::Unsupported);
    }

    fn wait_until(what: &str, f: impl Fn() -> bool) {
        let start = Instant::now();
        while !f() {
            assert!(start.elapsed() < Duration::from_secs(3), "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn one_thread_ticks_while_world_parts_run_and_none_is_left_behind() {
        let w = SimWorld::new();
        let router = Arc::new(Router::new());
        w.attach(Arc::clone(&router));
        let clock: SharedClock = Arc::new(MonotonicClock::new());
        assert_eq!(w.live_threads(), 0, "nothing to tick yet");
        let lidar = Arc::new(Recorder::default());
        let l = w.start(spec(lidar_info(), Mount::default(), "", &lidar, clock.clone()));
        let base = Arc::new(Recorder::default());
        let b = w.start(spec(base_info(), Mount::default(), "", &base, clock.clone()));
        assert_eq!(w.live_threads(), 1, "one thread for the whole world");
        assert!(router.subscribe(&pose_address()).is_some(), "world/pose exists while the world runs");
        wait_until("scans", || lidar.readings().len() >= 2);
        wait_until("base readings", || base.bases().len() >= 2);
        // Restart the base (its config changed): still one thread.
        drop(b);
        assert_eq!(w.live_threads(), 1);
        let b = w.start(spec(base_info(), Mount::default(), "", &base, clock.clone()));
        assert_eq!(w.live_threads(), 1);
        drop(b);
        drop(l);
        assert_eq!(w.live_threads(), 0, "the last part's removal stops and joins the thread");
        assert!(router.subscribe(&pose_address()).is_none(), "and world/pose goes with it");
        let n = lidar.readings().len();
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(lidar.readings().len(), n, "nothing publishes after removal");
        let again = w.start(spec(lidar_info(), Mount::default(), "", &lidar, clock));
        assert_eq!(w.live_threads(), 1, "and it starts again");
        drop(again);
        assert_eq!(w.live_threads(), 0);
    }
}
