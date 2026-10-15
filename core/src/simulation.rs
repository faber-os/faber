//! Layer 2: simulated parts (D36, docs/simulation.md, sections 4 and 8).
//!
//! A simulated part has the same name and capabilities as the real one; only its source changes.
//! This module holds
//! - **the seam** every kind of simulation plugs into: [`SimulatedPart`] (a running simulated
//!   part), [`Outlet`] (where its readings and health go) and [`PartSpec`] (what starts one).
//!   `simulate = "world"` parts run in the world engine (`simparts`), on top of this seam;
//! - **manual** simulated parts (`simulate = "manual"`): sensors publish a value or a
//!   [`Pattern`] at their rate, actuators report the commands they get as their state.
//!
//! Nothing here knows about leases or clamping: the machine checks the lease and clamps every
//! command to the part's info before a simulated part sees it, exactly as for a real part.

use crate::clock::SharedClock;
use crate::names::PartName;
use faber_capabilities::{
    Capability, Command, Info, Mount, Reading, Twist2d, distance, laser_scan, motor, planar_base,
};
use faber_connector::{Applied, CommandError, Health, Problem, Provide, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Where a simulated part's readings and health go. The machine implements it on top of the
/// router and the parts' status board, and declares the part's signals before it starts.
pub trait Outlet: Send + Sync {
    /// Publishes on one of the part's `provides` names.
    fn publish(&self, name: &str, reading: Reading);
    fn set_health(&self, health: Health);
}

/// Everything needed to start one simulated part.
#[derive(Clone)]
pub struct PartSpec {
    pub name: PartName,
    pub connector: String,
    /// What it provides, with the info (limits) its settings give.
    pub provides: Vec<Provide>,
    pub mount: Mount,
    pub settings: Settings,
    pub outlet: Arc<dyn Outlet>,
    pub clock: SharedClock,
}

/// A running simulated part. Dropping it stops it.
pub trait SimulatedPart: Send {
    /// A command for one of its actuators, already lease-checked and clamped by the machine.
    /// The safe state arrives the same way.
    fn command(&mut self, command: Command) -> Result<Applied, CommandError>;

    /// Sets the value pattern of a manual sensor.
    fn set_pattern(&mut self, pattern: Pattern) -> Result<(), PatternError> {
        let _ = pattern;
        Err(PatternError::NotManual)
    }

    /// The pattern a manual sensor follows now.
    fn pattern(&self) -> Option<Pattern> {
        None
    }
}

// ---- patterns ---------------------------------------------------------------------------------

/// How a manual sensor's value changes over time, in the capability's own unit (m for
/// `distance` and `laser-scan`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    Steady {
        value: f32,
    },
    /// `center ± amplitude`, one full wave every `period_s`.
    Sine {
        center: f32,
        amplitude: f32,
        period_s: f32,
    },
    /// `center ± amplitude`, random.
    Noise {
        center: f32,
        amplitude: f32,
    },
    /// A square wave: `low` for half of `period_s`, then `high`.
    Step {
        low: f32,
        high: f32,
        period_s: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternError {
    /// The part is not `simulate = "manual"`.
    NotManual,
    /// The part has no sensor whose value a pattern could set (e.g. a motor).
    NotASensor,
    /// A number is out of range; the message says which.
    Invalid(String),
}

impl Pattern {
    pub fn validate(&self) -> Result<(), PatternError> {
        let finite = |name: &str, v: f32| {
            if v.is_finite() {
                Ok(())
            } else {
                Err(PatternError::Invalid(format!("{name} must be a number.")))
            }
        };
        let period = |p: f32| {
            if p.is_finite() && p > 0.0 {
                Ok(())
            } else {
                Err(PatternError::Invalid("periodS must be more than 0 seconds.".into()))
            }
        };
        match *self {
            Pattern::Steady { value } => finite("value", value),
            Pattern::Sine { center, amplitude, period_s } => {
                finite("center", center)?;
                finite("amplitude", amplitude)?;
                period(period_s)
            }
            Pattern::Noise { center, amplitude } => {
                finite("center", center)?;
                finite("amplitude", amplitude)
            }
            Pattern::Step { low, high, period_s } => {
                finite("low", low)?;
                finite("high", high)?;
                period(period_s)
            }
        }
    }

    /// The value `t_s` seconds after the pattern was set. `random` is in -1..1.
    pub fn value_at(&self, t_s: f64, random: f32) -> f32 {
        match *self {
            Pattern::Steady { value } => value,
            Pattern::Sine { center, amplitude, period_s } => {
                center + amplitude * (std::f64::consts::TAU * t_s / f64::from(period_s)).sin() as f32
            }
            Pattern::Noise { center, amplitude } => center + amplitude * random,
            Pattern::Step { low, high, period_s } => {
                if (t_s / f64::from(period_s)).fract() < 0.5 {
                    low
                } else {
                    high
                }
            }
        }
    }
}

/// A small random number generator (xorshift); good enough for sensor noise, no dependency.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }
}

// ---- manual parts -----------------------------------------------------------------------------

/// Points in a manual laser scan: 0.8° apart, like the LD19.
const SCAN_POINTS: usize = 450;

/// One capability of a manual part, and its state.
enum Channel {
    Sensor {
        name: String,
        capability: Capability,
        info: Option<Info>,
        pattern: Pattern,
        since_us: u64,
    },
    Motor {
        name: String,
        info: motor::Info,
        velocity: f32,
        position: f32,
    },
    Base {
        name: String,
        info: planar_base::Info,
        commanded: Twist2d,
        measured: Twist2d,
    },
    /// Can't be set by hand (e.g. `pose`): publishes nothing.
    Silent,
}

struct Manual {
    channels: Vec<Channel>,
    last_tick_us: u64,
    noise: Noise,
}

/// A running manual part: a thread that publishes at the part's rate.
struct ManualPart {
    shared: Arc<Mutex<Manual>>,
    outlet: Arc<dyn Outlet>,
    clock: SharedClock,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Starts a `simulate = "manual"` part. Its `rate-hz` setting (if any) says how often it
/// publishes; the default suits the capability.
pub fn start_manual(spec: PartSpec) -> Box<dyn SimulatedPart> {
    let now = spec.clock.now_us();
    let channels: Vec<Channel> = spec.provides.iter().map(|p| channel(p, now)).collect();
    let rate = spec
        .settings
        .get("rate-hz")
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .filter(|r| *r > 0.0)
        .unwrap_or_else(|| default_rate(&spec.provides));
    let silent = channels.iter().all(|c| matches!(c, Channel::Silent));
    spec.outlet.set_health(if silent {
        Health::Degraded(Problem::new(
            "This part's values can't be set by hand.",
            "Use simulate = \"world\" to get its values from the simulated world.",
        ))
    } else {
        Health::Running
    });
    let seed = (now ^ ((spec.name.as_str().len() as u64) << 32)) | 1;
    let shared = Arc::new(Mutex::new(Manual { channels, last_tick_us: now, noise: Noise(seed) }));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let (shared, outlet, clock, stop) =
            (Arc::clone(&shared), Arc::clone(&spec.outlet), Arc::clone(&spec.clock), Arc::clone(&stop));
        let period = Duration::from_secs_f64(1.0 / rate);
        std::thread::Builder::new()
            .name(format!("sim:{}", spec.name))
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let now = clock.now_us();
                    // Published under the lock, so a tick can never publish an older state
                    // after a command has published a newer one.
                    let mut m = shared.lock().expect("manual part lock");
                    for (name, r) in m.tick(now) {
                        outlet.publish(&name, r);
                    }
                    drop(m);
                    sleep_unless_stopped(period, &stop);
                }
            })
            .expect("spawn simulated part")
    };
    Box::new(ManualPart { shared, outlet: spec.outlet, clock: spec.clock, stop, thread: Some(thread) })
}

fn default_rate(provides: &[Provide]) -> f64 {
    let actuator = provides.iter().any(|p| p.capability.lease().is_some());
    if actuator { 20.0 } else { 10.0 }
}

fn sleep_unless_stopped(d: Duration, stop: &AtomicBool) {
    let until = std::time::Instant::now() + d;
    while !stop.load(Ordering::Relaxed) {
        let left = until.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(Duration::from_millis(10)));
    }
}

fn channel(p: &Provide, now_us: u64) -> Channel {
    let name = p.name.clone();
    match (&p.capability, &p.info) {
        (Capability::Motor, Some(Info::Motor(info))) => {
            Channel::Motor { name, info: info.clone(), velocity: 0.0, position: 0.0 }
        }
        (Capability::PlanarBase, Some(Info::PlanarBase(info))) => {
            Channel::Base { name, info: info.clone(), commanded: Twist2d::ZERO, measured: Twist2d::ZERO }
        }
        (Capability::Distance | Capability::LaserScan, info) => Channel::Sensor {
            name,
            capability: p.capability,
            info: info.clone(),
            pattern: Pattern::Steady { value: default_value(info.as_ref()) },
            since_us: now_us,
        },
        _ => Channel::Silent,
    }
}

/// A value in the middle of what the sensor can see.
fn default_value(info: Option<&Info>) -> f32 {
    match info {
        Some(Info::Distance(i)) => (i.min_m + i.max_m) / 2.0,
        Some(Info::LaserScan(i)) => 2.0f32.clamp(i.range_min_m, i.range_max_m),
        _ => 1.0,
    }
}

impl Manual {
    /// Moves time on to `now_us` and returns what to publish.
    fn tick(&mut self, now_us: u64) -> Vec<(String, Reading)> {
        let dt = now_us.saturating_sub(self.last_tick_us) as f32 / 1e6;
        self.last_tick_us = now_us;
        let mut out = Vec::new();
        for c in &mut self.channels {
            match c {
                Channel::Sensor { name, capability, info, pattern, since_us } => {
                    let t = now_us.saturating_sub(*since_us) as f64 / 1e6;
                    let v = pattern.value_at(t, self.noise.next());
                    out.push((name.clone(), sensor_reading(*capability, info.as_ref(), now_us, v)));
                }
                Channel::Motor { name, velocity, position, .. } => {
                    *position += *velocity * dt;
                    out.push((name.clone(), motor_reading(now_us, *velocity, *position)));
                }
                Channel::Base { name, info, commanded, measured } => {
                    *measured = ramp(*measured, *commanded, info.max_acceleration, dt);
                    out.push((name.clone(), base_reading(now_us, *commanded, *measured)));
                }
                Channel::Silent => {}
            }
        }
        out
    }

    /// Applies a command and returns the state to publish right away.
    fn command(&mut self, command: Command, now_us: u64) -> Result<(String, Reading), CommandError> {
        for c in &mut self.channels {
            match (c, command) {
                (Channel::Motor { name, info, velocity, position }, Command::Motor(m)) => {
                    match m {
                        motor::Command::SetVelocity { velocity_rad_s } => *velocity = velocity_rad_s,
                        // Open loop: the simulated motor turns in proportion to its duty.
                        motor::Command::SetDuty { duty } => {
                            *velocity = duty * info.max_velocity_rad_s.unwrap_or(0.0)
                        }
                        motor::Command::SetPosition { position_rad } => {
                            *position = position_rad;
                            *velocity = 0.0;
                        }
                    }
                    return Ok((name.clone(), motor_reading(now_us, *velocity, *position)));
                }
                (
                    Channel::Base { name, commanded, measured, .. },
                    Command::PlanarBase(planar_base::Command::SetVelocity(t)),
                ) => {
                    *commanded = t;
                    return Ok((name.clone(), base_reading(now_us, *commanded, *measured)));
                }
                _ => {}
            }
        }
        Err(CommandError::Unsupported)
    }
}

/// Moves `from` towards `to`, by at most `accel × dt` on each axis.
fn ramp(from: Twist2d, to: Twist2d, accel: Twist2d, dt: f32) -> Twist2d {
    let step = |f: f32, t: f32, a: f32| f + (t - f).clamp(-a * dt, a * dt);
    Twist2d::new(
        step(from.vx, to.vx, accel.vx),
        step(from.vy, to.vy, accel.vy),
        step(from.omega, to.omega, accel.omega),
    )
}

fn motor_reading(t_us: u64, velocity: f32, position: f32) -> Reading {
    Reading::Motor(motor::Reading {
        t_us,
        velocity_rad_s: Some(velocity),
        position_rad: Some(position),
        current_a: None,
    })
}

fn base_reading(t_us: u64, commanded: Twist2d, measured: Twist2d) -> Reading {
    Reading::PlanarBase(planar_base::Reading { t_us, commanded, measured: Some(measured), bumped: false })
}

fn sensor_reading(capability: Capability, info: Option<&Info>, t_us: u64, value: f32) -> Reading {
    match (capability, info) {
        (Capability::Distance, Some(Info::Distance(i))) => Reading::Distance(i.reading(t_us, value)),
        (Capability::Distance, _) => Reading::Distance(distance::Reading {
            t_us,
            meters: value.max(0.0),
            status: distance::Status::Ok,
        }),
        _ => {
            let (min, max) = match info {
                Some(Info::LaserScan(i)) => (i.range_min_m, i.range_max_m),
                _ => (0.0, f32::INFINITY),
            };
            // A ring of points at the same range; out of range means no return.
            let range = if (min..=max).contains(&value) { value } else { f32::INFINITY };
            let step = std::f32::consts::TAU / SCAN_POINTS as f32;
            Reading::LaserScan(laser_scan::Reading::even_bins(t_us, 0.0, step, 0.1, vec![range; SCAN_POINTS]))
        }
    }
}

impl SimulatedPart for ManualPart {
    fn command(&mut self, command: Command) -> Result<Applied, CommandError> {
        let now = self.clock.now_us();
        let mut m = self.shared.lock().expect("manual part lock");
        let (name, reading) = m.command(command, now)?;
        self.outlet.publish(&name, reading);
        Ok(Applied { command })
    }

    fn set_pattern(&mut self, pattern: Pattern) -> Result<(), PatternError> {
        pattern.validate()?;
        let now = self.clock.now_us();
        let mut m = self.shared.lock().expect("manual part lock");
        let mut found = false;
        for c in &mut m.channels {
            if let Channel::Sensor { pattern: p, since_us, .. } = c {
                *p = pattern;
                *since_us = now;
                found = true;
            }
        }
        if found { Ok(()) } else { Err(PatternError::NotASensor) }
    }

    fn pattern(&self) -> Option<Pattern> {
        self.shared.lock().expect("manual part lock").channels.iter().find_map(|c| match c {
            Channel::Sensor { pattern, .. } => Some(*pattern),
            _ => None,
        })
    }
}

impl Drop for ManualPart {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;

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
        fn last(&self) -> Option<Reading> {
            self.published.lock().unwrap().last().map(|(_, r)| r.clone())
        }
    }

    fn base_info() -> planar_base::Info {
        planar_base::Info {
            holonomic: true,
            max_velocity: Twist2d::new(1.0, 1.0, 2.0),
            max_acceleration: Twist2d::new(1.0, 1.0, 2.0),
            built_from: vec![],
        }
    }

    fn distance_info() -> distance::Info {
        distance::Info { min_m: 0.1, max_m: 2.0, field_of_view_rad: 0.2, kind: distance::Kind::Infrared }
    }

    fn spec(info: Info, outlet: Arc<Recorder>, clock: Arc<ManualClock>, rate: f64) -> PartSpec {
        let mut settings = Settings::new();
        settings.insert("rate-hz".into(), rate.into());
        PartSpec {
            name: PartName::parse("part").unwrap(),
            connector: "x".into(),
            provides: vec![Provide::with_info(info)],
            mount: Mount::default(),
            settings,
            outlet,
            clock,
        }
    }

    fn manual(info: Info) -> Manual {
        Manual { channels: vec![channel(&Provide::with_info(info), 0)], last_tick_us: 0, noise: Noise(1) }
    }

    #[test]
    fn patterns_follow_their_shape() {
        assert_eq!(Pattern::Steady { value: 1.5 }.value_at(10.0, 0.9), 1.5);
        let sine = Pattern::Sine { center: 1.0, amplitude: 0.5, period_s: 4.0 };
        assert!((sine.value_at(1.0, 0.0) - 1.5).abs() < 1e-5, "a quarter wave in");
        assert!((sine.value_at(3.0, 0.0) - 0.5).abs() < 1e-5);
        let step = Pattern::Step { low: 0.2, high: 1.0, period_s: 2.0 };
        assert_eq!(
            (step.value_at(0.5, 0.0), step.value_at(1.5, 0.0), step.value_at(2.5, 0.0)),
            (0.2, 1.0, 0.2)
        );
        let noise = Pattern::Noise { center: 1.0, amplitude: 0.1 };
        assert!((noise.value_at(0.0, -1.0) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn bad_patterns_are_refused() {
        assert!(Pattern::Sine { center: 1.0, amplitude: 1.0, period_s: 0.0 }.validate().is_err());
        assert!(Pattern::Steady { value: f32::NAN }.validate().is_err());
        assert!(Pattern::Step { low: 0.0, high: 1.0, period_s: 1.0 }.validate().is_ok());
    }

    #[test]
    fn noise_stays_within_one() {
        let mut n = Noise(12345);
        let values: Vec<f32> = (0..1000).map(|_| n.next()).collect();
        assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)));
        assert!(values.iter().any(|v| *v > 0.5) && values.iter().any(|v| *v < -0.5), "spread out");
    }

    #[test]
    fn a_manual_distance_sensor_reports_its_pattern_within_its_range() {
        let mut m = manual(Info::Distance(distance_info()));
        let Reading::Distance(r) = &m.tick(100).remove(0).1 else { panic!() };
        assert_eq!((r.meters, r.status), (1.05, distance::Status::Ok), "starts in the middle of its range");
        if let Channel::Sensor { pattern, .. } = &mut m.channels[0] {
            *pattern = Pattern::Steady { value: 5.0 };
        }
        let Reading::Distance(r) = &m.tick(200).remove(0).1 else { panic!() };
        assert_eq!((r.meters, r.status), (2.0, distance::Status::TooFar));
    }

    #[test]
    fn a_manual_laser_scan_is_a_ring_at_the_value() {
        let info = Info::LaserScan(laser_scan::Info { range_min_m: 0.02, range_max_m: 12.0 });
        let mut m = manual(info);
        let Reading::LaserScan(r) = &m.tick(1).remove(0).1 else { panic!() };
        assert_eq!(r.len(), SCAN_POINTS);
        assert!(r.ranges.iter().all(|x| *x == 2.0));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn a_base_reports_what_it_was_commanded_and_ramps_its_measured_velocity() {
        let mut m = manual(Info::PlanarBase(base_info()));
        let cmd = Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.5, 0.0, 0.0)));
        let (_, Reading::PlanarBase(r)) = m.command(cmd, 0).unwrap() else { panic!() };
        assert_eq!(r.commanded.vx, 0.5);
        assert_eq!(r.measured.unwrap().vx, 0.0, "it can't jump");
        let Reading::PlanarBase(r) = &m.tick(200_000).remove(0).1 else { panic!() };
        assert!((r.measured.unwrap().vx - 0.2).abs() < 1e-5, "1 m/s² for 0.2 s");
        let Reading::PlanarBase(r) = &m.tick(1_000_000).remove(0).1 else { panic!() };
        assert_eq!(r.measured.unwrap().vx, 0.5, "and stops at what was commanded");
    }

    #[test]
    fn a_motor_reports_its_command_and_integrates_its_position() {
        let info = motor::Info {
            kind: motor::Kind::Dc,
            modes: motor::Modes { velocity: true, position: true, duty: true },
            max_velocity_rad_s: Some(10.0),
        };
        let mut m = manual(Info::Motor(info));
        m.command(Command::Motor(motor::Command::SetDuty { duty: 0.5 }), 0).unwrap();
        let Reading::Motor(r) = &m.tick(1_000_000).remove(0).1 else { panic!() };
        assert_eq!(r.velocity_rad_s, Some(5.0));
        assert!((r.position_rad.unwrap() - 5.0).abs() < 1e-4);
        let (_, Reading::Motor(r)) =
            m.command(Command::Motor(motor::Command::SetPosition { position_rad: 1.0 }), 2).unwrap()
        else {
            panic!()
        };
        assert_eq!((r.velocity_rad_s, r.position_rad), (Some(0.0), Some(1.0)));
    }

    #[test]
    fn commands_for_a_capability_the_part_lacks_are_unsupported() {
        let mut m = manual(Info::Distance(distance_info()));
        let stop = Command::PlanarBase(planar_base::Command::STOP);
        assert_eq!(m.command(stop, 0).unwrap_err(), CommandError::Unsupported);
    }

    fn wait_until(f: impl Fn() -> bool) {
        let start = std::time::Instant::now();
        while !f() {
            assert!(start.elapsed() < Duration::from_secs(2), "timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn a_running_manual_part_publishes_at_its_rate_and_takes_patterns() {
        let outlet = Arc::new(Recorder::default());
        let clock = Arc::new(ManualClock::new(0));
        let mut part = start_manual(spec(
            Info::Distance(distance_info()),
            Arc::clone(&outlet),
            Arc::clone(&clock),
            200.0,
        ));
        assert_eq!(outlet.health.lock().unwrap().last(), Some(&Health::Running));
        wait_until(|| outlet.published.lock().unwrap().len() >= 3);
        part.set_pattern(Pattern::Steady { value: 0.5 }).unwrap();
        assert_eq!(part.pattern(), Some(Pattern::Steady { value: 0.5 }));
        wait_until(|| matches!(outlet.last(), Some(Reading::Distance(r)) if r.meters == 0.5));
        assert_eq!(
            part.set_pattern(Pattern::Steady { value: f32::NAN }).unwrap_err(),
            PatternError::Invalid("value must be a number.".into())
        );
        drop(part);
        let n = outlet.published.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(outlet.published.lock().unwrap().len(), n, "dropping stops it");
    }

    #[test]
    fn an_actuator_publishes_a_command_at_once_and_has_no_pattern() {
        let outlet = Arc::new(Recorder::default());
        let clock = Arc::new(ManualClock::new(0));
        let mut part = start_manual(spec(Info::PlanarBase(base_info()), Arc::clone(&outlet), clock, 1.0));
        let cmd = Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.3, 0.0, 0.0)));
        assert_eq!(part.command(cmd).unwrap(), Applied { command: cmd });
        let Some(Reading::PlanarBase(r)) = outlet.last() else { panic!() };
        assert_eq!(r.commanded.vx, 0.3);
        assert_eq!(part.set_pattern(Pattern::Steady { value: 1.0 }), Err(PatternError::NotASensor));
        assert_eq!(part.pattern(), None);
    }
}
