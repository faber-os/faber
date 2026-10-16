//! Generic connectors (docs/simulation.md, section 3): a motor, a drive base, a distance sensor,
//! a battery, an obstacle sensor, a servo, an encoder and an IMU described by their settings alone. Real ones sit on a FaberLink board and appear as its
//! sub-parts by themselves (`faber-bridge`), so a generic part is only ever simulated; their
//! manifests say so with [`PortRequirement::Bridge`].
//!
//! The settings are the part's limits, which become its capability info: the core clamps every
//! command to them, and simulations respect them. Each manifest declares its settings
//! ([`SettingSpec`]); the shell builds its form from them and a [`Reader`] reads them.

use faber_capabilities::{
    Capability, Info, Twist2d, battery, distance, encoder, motor, obstacle, planar_base, servo,
};
use faber_connector::settings::Reader;
use faber_connector::{
    Choice, Connector, Context, Definition, Manifest, PortRequirement, Provide, SettingSpec, Settings,
    SettingsError,
};

pub const PLANAR_BASE: &str = "generic-planar-base";
pub const MOTOR: &str = "generic-motor";
pub const DISTANCE: &str = "generic-distance";
pub const BATTERY: &str = "generic-battery";
pub const OBSTACLE: &str = "generic-obstacle";
pub const SERVO: &str = "generic-servo";
pub const ENCODER: &str = "generic-encoder";
pub const IMU: &str = "generic-imu";

pub fn definitions() -> Vec<Definition> {
    vec![
        definition(PLANAR_BASE, "Generic drive base", base_settings(), base_provides),
        definition(MOTOR, "Generic motor", motor_settings(), motor_provides),
        definition(DISTANCE, "Generic distance sensor", distance_settings(), distance_provides),
        definition(BATTERY, "Generic battery", battery_settings(), battery_provides),
        definition(OBSTACLE, "Generic obstacle sensor (bumper)", obstacle_settings(), obstacle_provides),
        definition(SERVO, "Generic servo", servo_settings(), servo_provides),
        definition(ENCODER, "Generic encoder", encoder_settings(), encoder_provides),
        definition(IMU, "Generic IMU", imu_settings(), imu_provides),
    ]
}

fn definition(
    id: &str,
    name: &str,
    settings: Vec<SettingSpec>,
    provides: fn(&Manifest, &Settings) -> Result<Vec<Provide>, SettingsError>,
) -> Definition {
    let mut manifest = Manifest {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PortRequirement::Bridge,
        provides: Vec::new(),
        settings,
        usb_hints: Vec::new(),
    };
    manifest.provides = provides(&manifest, &Settings::new()).expect("the defaults are valid");
    let create = match id {
        PLANAR_BASE => create_base,
        MOTOR => create_motor,
        BATTERY => create_battery,
        OBSTACLE => create_obstacle,
        SERVO => create_servo,
        ENCODER => create_encoder,
        IMU => create_imu,
        _ => create_distance,
    };
    Definition::new(manifest, create).with_provides(provides)
}

/// A drive base's `length` × `width` (m) when its settings don't say.
pub const DEFAULT_FOOTPRINT: (f64, f64) = (0.5, 0.4);

/// How often a simulated part publishes. Every generic part takes it.
fn rate(default_hz: f64) -> SettingSpec {
    SettingSpec::number("rate-hz", "Rate", "How often a simulated part sends a new value.", "Hz", default_hz)
        .positive()
        .advanced()
}

/// The defaults are what an old parts.toml without these settings has always meant, so they
/// stay. A new part's form starts at SENNEP's footprint (0.8 × 0.65 m) and limits (0.6 m/s).
fn base_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::number(
            "length",
            "Length",
            "Front to back. The simulated world uses it as the machine's footprint.",
            "m",
            DEFAULT_FOOTPRINT.0,
        )
        .positive()
        .suggested(0.8),
        SettingSpec::number("width", "Width", "Side to side, wheels included.", "m", DEFAULT_FOOTPRINT.1)
            .positive()
            .suggested(0.65),
        SettingSpec::bool(
            "holonomic",
            "Can drive sideways (mecanum or omni wheels)",
            "Off for a base that only drives forward, backward and turns, like a tank.",
            true,
        ),
        SettingSpec::number(
            "max-vx",
            "Top speed forward",
            "Faster commands are slowed down to this.",
            "m/s",
            0.5,
        )
        .positive()
        .suggested(0.6),
        SettingSpec::number(
            "max-vy",
            "Top speed sideways",
            "Only used when it can drive sideways.",
            "m/s",
            0.5,
        )
        .positive()
        .suggested(0.6),
        SettingSpec::number(
            "max-omega",
            "Top turning speed",
            "How fast it may turn on the spot (1.5 rad/s is about a quarter turn a second).",
            "rad/s",
            1.5,
        )
        .positive(),
        SettingSpec::number(
            "max-accel",
            "Acceleration",
            "How quickly it changes speed. Lower is gentler on the wheels.",
            "m/s²",
            1.0,
        )
        .positive(),
        rate(20.0),
        SettingSpec::part_list(
            "built-from",
            "Built from motors",
            "The motor parts it drives with. Driving the base locks them.",
        )
        .advanced(),
    ]
}

fn motor_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::number(
            "max-velocity",
            "Top speed",
            "The fastest it may turn its shaft (10 rad/s is about 95 rpm).",
            "rad/s",
            10.0,
        )
        .positive(),
        SettingSpec::choice(
            "kind",
            "Kind of motor",
            "What drives it; shown in its info.",
            "dc",
            vec![
                Choice::new("dc", "DC motor"),
                Choice::new("stepper", "Stepper"),
                Choice::new("bldc", "Brushless (BLDC)"),
                Choice::new("continuous-servo", "Continuous servo"),
            ],
        ),
        rate(20.0),
    ]
}

fn distance_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::number(
            "min-m",
            "Closest it can measure",
            "Anything nearer reads as too close. From the sensor's datasheet.",
            "m",
            0.02,
        )
        .at_least(0.0),
        SettingSpec::number(
            "max-m",
            "Farthest it can measure",
            "Anything farther reads as too far. From the sensor's datasheet.",
            "m",
            4.0,
        )
        .positive(),
        SettingSpec::number(
            "fov-rad",
            "Field of view",
            "How wide its cone is (0.26 rad is 15°).",
            "rad",
            0.26,
        )
        .positive(),
        SettingSpec::choice(
            "kind",
            "Kind of sensor",
            "How it measures; shown in its info.",
            "ultrasonic",
            vec![
                Choice::new("ultrasonic", "Ultrasonic"),
                Choice::new("laser-tof", "Laser time-of-flight"),
                Choice::new("infrared", "Infrared"),
                Choice::new("radar", "Radar"),
            ],
        ),
        rate(10.0),
    ]
}

/// A bumper by default: something touches it. An infrared module's trigger distance is set with
/// its potentiometer; the simulated world uses it.
fn obstacle_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::choice(
            "kind",
            "Kind of sensor",
            "How it senses; shown in its info.",
            "bumper",
            vec![
                Choice::new("bumper", "Bumper or whisker"),
                Choice::new("infrared", "Infrared obstacle module"),
                Choice::new("limit", "Limit switch"),
            ],
        ),
        SettingSpec::number(
            "trigger-distance-m",
            "Detects within",
            "How close something must come before it is detected: about 0.01 m for a bumper, what its potentiometer is set to for an infrared module.",
            "m",
            0.01,
        )
        .at_least(0.0),
        SettingSpec::number(
            "fov-rad",
            "Field of view",
            "How wide the area it watches is (0.5 rad is about 29°). Used by the simulated world.",
            "rad",
            0.5,
        )
        .positive()
        .advanced(),
        rate(10.0),
    ]
}

/// An SG90-sized servo: 0 to 180°, holding its angle when its lease ends (D29).
fn servo_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::number("min-angle", "Smallest angle", "The angle it may go down to.", "rad", 0.0),
        SettingSpec::number(
            "max-angle",
            "Largest angle",
            "The angle it may go up to (3.14 rad is 180°).",
            "rad",
            std::f64::consts::PI,
        ),
        SettingSpec::choice(
            "safe-state",
            "When nobody drives it",
            "What it does when its lease ends or the e-stop is pressed.",
            "hold",
            vec![Choice::new("hold", "Hold its angle"), Choice::new("release", "Go limp (release)")],
        ),
        rate(10.0),
    ]
}

fn encoder_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::number(
            "counts-per-rev",
            "Counts per turn",
            "Counts for one turn of the shaft it measures, gearbox included. 0 if you don't know: then its speed is in counts per second only.",
            "",
            0.0,
        )
        .at_least(0.0),
        rate(20.0),
    ]
}

fn imu_settings() -> Vec<SettingSpec> {
    vec![rate(20.0)]
}

/// The defaults are a 3S Li-ion pack of 2.2 Ah, a common size for small robots. The currents
/// only matter in the simulated world, where the battery drains as the machine drives.
fn battery_settings() -> Vec<SettingSpec> {
    vec![
        SettingSpec::choice(
            "chemistry",
            "Kind of battery",
            "What its cells are made of. It sets when the battery counts as low and critical.",
            "li-ion",
            vec![
                Choice::new("li-ion", "Li-ion (e.g. 18650 cells)"),
                Choice::new("lipo", "LiPo"),
                Choice::new("lifepo4", "LiFePO4"),
                Choice::new("nimh", "NiMH"),
                Choice::new("lead-acid", "Lead-acid"),
            ],
        ),
        SettingSpec::number(
            "cells-in-series",
            "Cells in series",
            "The 3 in \"3S\": how many cells add up to its voltage.",
            "",
            3.0,
        )
        .at_least(1.0)
        .at_most(24.0),
        SettingSpec::number("capacity-ah", "Capacity", "How much it holds, from its label.", "Ah", 2.2).positive(),
        SettingSpec::number(
            "idle-current-a",
            "Current when standing still",
            "What the machine draws while it stands still (the computer, the sensors). Used by the simulated world.",
            "A",
            0.5,
        )
        .at_least(0.0),
        SettingSpec::number(
            "moving-current-a",
            "Extra current at full speed",
            "What driving at top speed adds on top. Used by the simulated world.",
            "A",
            3.0,
        )
        .at_least(0.0),
        SettingSpec::number(
            "start-charge",
            "Charge at start",
            "How full it starts in the simulated world: 1 is full.",
            "",
            1.0,
        )
        .at_least(0.0)
        .at_most(1.0)
        .advanced(),
        rate(2.0),
    ]
}

/// How a world battery drains: the currents (A) and its charge (0-1) when it starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drain {
    pub idle_a: f32,
    pub moving_a: f32,
    pub start_charge: f32,
}

/// A generic battery's drain settings (docs/simulation.md, section 3). Settings were checked when
/// the part was added; anything unreadable falls back to the defaults.
pub fn battery_drain(s: &Settings) -> Drain {
    let read = |key: &str, default: f32| {
        s.get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .map(|n| n as f32)
            .filter(|n| n.is_finite() && *n >= 0.0)
            .unwrap_or(default)
    };
    Drain {
        idle_a: read("idle-current-a", 0.5),
        moving_a: read("moving-current-a", 3.0),
        start_charge: read("start-charge", 1.0).min(1.0),
    }
}

/// A generic part has nothing to run until bridges arrive; creating one only checks its settings.
struct Generic;

impl Connector for Generic {
    fn on_bytes(&mut self, _: &mut dyn Context, _: u64, _: &[u8]) {}
}

fn create_base(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    base_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_motor(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    motor_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_distance(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    distance_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_battery(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    battery_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_obstacle(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    obstacle_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_servo(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    servo_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_encoder(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    encoder_info(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn create_imu(s: &Settings) -> Result<Box<dyn Connector>, SettingsError> {
    imu_check(s).map(|_| Box::new(Generic) as Box<dyn Connector>)
}

fn obstacle_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Obstacle(obstacle_info(s)?))])
}

fn servo_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Servo(servo_info(s)?))])
}

fn encoder_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Encoder(encoder_info(s)?))])
}

/// An IMU has no info: what it measures is all in its readings.
fn imu_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    imu_check(s)?;
    Ok(vec![Provide::default_for(Capability::Imu)])
}

fn base_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::PlanarBase(base_info(s)?))])
}

fn motor_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Motor(motor_info(s)?))])
}

fn distance_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Distance(distance_info(s)?))])
}

fn battery_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Battery(battery_info(s)?))])
}

/// A drive base's footprint, `length` × `width` (m), from its settings. The world shows and
/// collides it (docs/simulation.md, section 7). Settings were checked when the part was added;
/// anything unreadable falls back to [`DEFAULT_FOOTPRINT`].
pub fn footprint(s: &Settings) -> (f64, f64) {
    let read = |key: &str, default: f64| {
        s.get(key)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .filter(|n| n.is_finite() && *n > 0.0)
            .unwrap_or(default)
    };
    (read("length", DEFAULT_FOOTPRINT.0), read("width", DEFAULT_FOOTPRINT.1))
}

fn base_info(s: &Settings) -> Result<planar_base::Info, SettingsError> {
    let specs = base_settings();
    let r = Reader::new(s, &specs, "a generic drive base")?;
    let num = |key: &str| r.number(key).map(|n| n as f32);
    let max = Twist2d::new(num("max-vx")?, num("max-vy")?, num("max-omega")?);
    let accel = num("max-accel")?;
    num("length")?;
    num("width")?;
    num("rate-hz")?;
    // One acceleration setting: turning takes as long to reach full speed as driving does.
    let omega_accel = accel * max.omega / max.vx.max(max.vy);
    Ok(planar_base::Info {
        holonomic: r.bool("holonomic")?,
        max_velocity: max,
        max_acceleration: Twist2d::new(accel, accel, omega_accel),
        built_from: r.part_list("built-from")?,
    })
}

fn motor_info(s: &Settings) -> Result<motor::Info, SettingsError> {
    let specs = motor_settings();
    let r = Reader::new(s, &specs, "a generic motor")?;
    r.number("rate-hz")?;
    let kind = r.choice("kind")?;
    Ok(motor::Info {
        kind: motor::Kind::ALL.iter().copied().find(|k| k.id() == kind).expect("declared kinds exist"),
        modes: motor::Modes { velocity: true, position: false, duty: true },
        max_velocity_rad_s: Some(r.number("max-velocity")? as f32),
    })
}

fn battery_info(s: &Settings) -> Result<battery::Info, SettingsError> {
    let specs = battery_settings();
    let r = Reader::new(s, &specs, "a generic battery")?;
    for key in ["idle-current-a", "moving-current-a", "start-charge", "rate-hz"] {
        r.number(key)?;
    }
    let cells = r.number("cells-in-series")?;
    if cells.fract() != 0.0 {
        return Err(SettingsError {
            key: "cells-in-series".into(),
            message: format!("cells-in-series = {cells} is not a whole number of cells."),
            hint: "Count the cells in series: a \"3S\" pack has cells-in-series = 3.".into(),
        });
    }
    let chemistry = r.choice("chemistry")?;
    Ok(battery::Info {
        chemistry: battery::Chemistry::from_id(chemistry).expect("declared chemistries exist"),
        cells_in_series: cells as u8,
        capacity_ah: Some(r.number("capacity-ah")? as f32),
    })
}

fn obstacle_info(s: &Settings) -> Result<obstacle::Info, SettingsError> {
    let specs = obstacle_settings();
    let r = Reader::new(s, &specs, "a generic obstacle sensor")?;
    r.number("rate-hz")?;
    r.number("fov-rad")?;
    let kind = r.choice("kind")?;
    Ok(obstacle::Info {
        kind: obstacle::Kind::ALL.iter().copied().find(|k| k.id() == kind).expect("declared kinds exist"),
        trigger_distance_m: Some(r.number("trigger-distance-m")? as f32),
    })
}

fn servo_info(s: &Settings) -> Result<servo::Info, SettingsError> {
    let specs = servo_settings();
    let r = Reader::new(s, &specs, "a generic servo")?;
    r.number("rate-hz")?;
    let (min, max) = (r.number("min-angle")? as f32, r.number("max-angle")? as f32);
    if min >= max {
        return Err(SettingsError {
            key: "min-angle".into(),
            message: format!("min-angle ({min} rad) must be less than max-angle ({max} rad)."),
            hint: "A 180° servo goes from min-angle = 0 to max-angle = 3.14159.".into(),
        });
    }
    let safe = r.choice("safe-state")?;
    Ok(servo::Info {
        min_angle_rad: min,
        max_angle_rad: max,
        safe_state: servo::SafeState::ALL
            .iter()
            .copied()
            .find(|k| k.id() == safe)
            .expect("declared states exist"),
    })
}

fn encoder_info(s: &Settings) -> Result<encoder::Info, SettingsError> {
    let specs = encoder_settings();
    let r = Reader::new(s, &specs, "a generic encoder")?;
    r.number("rate-hz")?;
    let cpr = r.number("counts-per-rev")? as f32;
    Ok(encoder::Info { counts_per_rev: (cpr > 0.0).then_some(cpr) })
}

fn imu_check(s: &Settings) -> Result<(), SettingsError> {
    let specs = imu_settings();
    Reader::new(s, &specs, "a generic IMU")?.number("rate-hz")?;
    Ok(())
}

fn distance_info(s: &Settings) -> Result<distance::Info, SettingsError> {
    let specs = distance_settings();
    let r = Reader::new(s, &specs, "a generic distance sensor")?;
    r.number("rate-hz")?;
    let kind = r.choice("kind")?;
    let (min_m, max_m) = (r.number("min-m")? as f32, r.number("max-m")? as f32);
    if min_m >= max_m {
        return Err(SettingsError {
            key: "min-m".into(),
            message: format!("min-m ({min_m} m) must be less than max-m ({max_m} m)."),
            hint: "Use the range from the sensor's datasheet, e.g. min-m = 0.02 and max-m = 4.0.".into(),
        });
    }
    Ok(distance::Info {
        min_m,
        max_m,
        field_of_view_rad: r.number("fov-rad")? as f32,
        kind: distance::Kind::ALL.iter().copied().find(|k| k.id() == kind).expect("declared kinds exist"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(text: &str) -> Settings {
        toml::from_str(text).unwrap()
    }

    fn info_of(id: &str, s: &Settings) -> Result<Info, SettingsError> {
        let d = definitions().into_iter().find(|d| d.manifest.id == id).unwrap();
        (d.create)(s)?;
        Ok((d.provides)(&d.manifest, s)?.remove(0).info.unwrap())
    }

    #[test]
    fn manifests_need_a_bridge_and_carry_default_limits() {
        for d in definitions() {
            assert_eq!(d.manifest.port, PortRequirement::Bridge, "{}", d.manifest.id);
            assert_eq!(d.manifest.validate(), Ok(()));
            let has_info = d.manifest.provides[0].info.is_some();
            assert_eq!(has_info, d.manifest.id != IMU, "only the IMU has no info: {}", d.manifest.id);
        }
        let caps: Vec<_> = definitions().iter().map(|d| d.manifest.provides[0].capability).collect();
        assert_eq!(
            caps,
            [
                Capability::PlanarBase,
                Capability::Motor,
                Capability::Distance,
                Capability::Battery,
                Capability::Obstacle,
                Capability::Servo,
                Capability::Encoder,
                Capability::Imu
            ]
        );
    }

    #[test]
    fn base_settings_from_the_design_become_its_info() {
        let s = settings(
            "length = 0.8\nwidth = 0.65\nmax-vx = 0.6\nmax-vy = 0.6\nmax-omega = 1.5\nmax-accel = 2\n",
        );
        let Info::PlanarBase(i) = info_of(PLANAR_BASE, &s).unwrap() else { panic!() };
        assert!(i.holonomic, "holonomic by default");
        assert_eq!(i.max_velocity, Twist2d::new(0.6, 0.6, 1.5));
        assert_eq!(i.max_acceleration.vx, 2.0);
        assert!((i.max_acceleration.omega - 5.0).abs() < 1e-5, "same time to full speed when turning");
        let s = settings("holonomic = false\nbuilt-from = [\"left\", \"right\"]\n");
        let Info::PlanarBase(i) = info_of(PLANAR_BASE, &s).unwrap() else { panic!() };
        assert!(!i.holonomic);
        assert_eq!(i.built_from, ["left", "right"]);
    }

    #[test]
    fn motor_and_distance_settings_become_their_info() {
        let Info::Motor(m) = info_of(MOTOR, &settings("max-velocity = 20\nkind = \"stepper\"\n")).unwrap()
        else {
            panic!()
        };
        assert_eq!((m.kind, m.max_velocity_rad_s), (motor::Kind::Stepper, Some(20.0)));
        assert!(m.modes.velocity && m.modes.duty);
        let s = settings("min-m = 0.03\nmax-m = 2\nfov-rad = 0.5\nkind = \"laser-tof\"\n");
        let Info::Distance(d) = info_of(DISTANCE, &s).unwrap() else { panic!() };
        assert_eq!(
            d,
            distance::Info {
                min_m: 0.03,
                max_m: 2.0,
                field_of_view_rad: 0.5,
                kind: distance::Kind::LaserTof
            }
        );
    }

    #[test]
    fn mistakes_name_the_setting_and_say_what_to_use() {
        let cases = [
            (PLANAR_BASE, "max-speed = 1", "max-speed"),
            (PLANAR_BASE, "max-vx = -1", "max-vx"),
            (PLANAR_BASE, "holonomic = \"yes\"", "holonomic"),
            (PLANAR_BASE, "built-from = \"fl\"", "built-from"),
            (MOTOR, "kind = \"diesel\"", "kind"),
            (DISTANCE, "min-m = 5", "min-m"),
            (DISTANCE, "max-m = \"far\"", "max-m"),
            (BATTERY, "chemistry = \"potato\"", "chemistry"),
            (BATTERY, "cells-in-series = 2.5", "cells-in-series"),
            (BATTERY, "cells-in-series = 0", "cells-in-series"),
            (BATTERY, "start-charge = 1.5", "start-charge"),
            (BATTERY, "capacity-ah = 0", "capacity-ah"),
            (OBSTACLE, "kind = \"radar\"", "kind"),
            (OBSTACLE, "trigger-distance-m = -1", "trigger-distance-m"),
            (SERVO, "min-angle = 2\nmax-angle = 1", "min-angle"),
            (SERVO, "safe-state = \"drop\"", "safe-state"),
            (ENCODER, "counts-per-rev = -5", "counts-per-rev"),
            (IMU, "rate-hz = 0", "rate-hz"),
        ];
        for (id, text, key) in cases {
            let e = info_of(id, &settings(text)).unwrap_err();
            assert_eq!(e.key, key, "{text}");
            assert!(!e.message.is_empty() && !e.hint.is_empty(), "{e:?}");
        }
        let e = info_of(PLANAR_BASE, &settings("max-speed = 1")).unwrap_err();
        assert!(e.hint.contains("max-vx"), "the hint lists the real settings: {e:?}");
    }

    #[test]
    fn a_base_footprint_comes_from_its_settings() {
        assert_eq!(footprint(&settings("length = 0.8\nwidth = 0.65\n")), (0.8, 0.65));
        assert_eq!(footprint(&settings("length = 1\n")), (1.0, DEFAULT_FOOTPRINT.1));
        assert_eq!(footprint(&Settings::new()), DEFAULT_FOOTPRINT);
    }

    #[test]
    fn manifests_declare_every_setting_they_read() {
        let keys = |id: &str| -> Vec<String> {
            let d = definitions().into_iter().find(|d| d.manifest.id == id).unwrap();
            d.manifest.settings.iter().map(|s| s.key.clone()).collect()
        };
        assert_eq!(
            keys(PLANAR_BASE),
            [
                "length",
                "width",
                "holonomic",
                "max-vx",
                "max-vy",
                "max-omega",
                "max-accel",
                "rate-hz",
                "built-from"
            ]
        );
        assert_eq!(keys(MOTOR), ["max-velocity", "kind", "rate-hz"]);
        assert_eq!(keys(DISTANCE), ["min-m", "max-m", "fov-rad", "kind", "rate-hz"]);
        assert_eq!(
            keys(BATTERY),
            [
                "chemistry",
                "cells-in-series",
                "capacity-ah",
                "idle-current-a",
                "moving-current-a",
                "start-charge",
                "rate-hz"
            ]
        );
    }

    #[test]
    fn a_choice_lists_every_kind_the_capability_has() {
        let choices = |id: &str| -> Vec<String> {
            let d = definitions().into_iter().find(|d| d.manifest.id == id).unwrap();
            let spec = d.manifest.settings.into_iter().find(|s| s.key == "kind").unwrap();
            let faber_connector::SettingKind::Choice { choices, .. } = spec.kind else { panic!() };
            choices.into_iter().map(|c| c.value).collect()
        };
        let obstacles: Vec<_> = obstacle::Kind::ALL.iter().map(|k| k.id().to_string()).collect();
        assert_eq!(choices(OBSTACLE), obstacles);
        let motors: Vec<_> = motor::Kind::ALL.iter().map(|k| k.id().to_string()).collect();
        let sensors: Vec<_> = distance::Kind::ALL.iter().map(|k| k.id().to_string()).collect();
        assert_eq!(choices(MOTOR), motors);
        assert_eq!(choices(DISTANCE), sensors);
        let chemistries: Vec<_> = battery::Chemistry::ALL.iter().map(|c| c.id().to_string()).collect();
        let d = definitions().into_iter().find(|d| d.manifest.id == BATTERY).unwrap();
        let spec = d.manifest.settings.into_iter().find(|s| s.key == "chemistry").unwrap();
        let faber_connector::SettingKind::Choice { choices, .. } = spec.kind else { panic!() };
        assert_eq!(choices.into_iter().map(|c| c.value).collect::<Vec<_>>(), chemistries);
    }

    /// An old parts.toml that leaves the settings out keeps meaning what it always meant.
    #[test]
    fn left_out_settings_keep_their_old_defaults() {
        let Info::PlanarBase(b) = info_of(PLANAR_BASE, &Settings::new()).unwrap() else { panic!() };
        assert_eq!((b.holonomic, b.max_velocity), (true, Twist2d::new(0.5, 0.5, 1.5)));
        assert_eq!(b.max_acceleration.vx, 1.0);
        assert_eq!(footprint(&Settings::new()), (0.5, 0.4));
        let Info::Motor(m) = info_of(MOTOR, &Settings::new()).unwrap() else { panic!() };
        assert_eq!((m.kind, m.max_velocity_rad_s), (motor::Kind::Dc, Some(10.0)));
        let Info::Distance(d) = info_of(DISTANCE, &Settings::new()).unwrap() else { panic!() };
        let old = distance::Info {
            min_m: 0.02,
            max_m: 4.0,
            field_of_view_rad: 0.26,
            kind: distance::Kind::Ultrasonic,
        };
        assert_eq!(d, old);
    }

    #[test]
    fn every_generic_part_accepts_a_rate() {
        for id in [PLANAR_BASE, MOTOR, DISTANCE, BATTERY, OBSTACLE, SERVO, ENCODER] {
            assert!(info_of(id, &settings("rate-hz = 5")).is_ok(), "{id}");
        }
        let imu = definitions().into_iter().find(|d| d.manifest.id == IMU).unwrap();
        assert!((imu.create)(&settings("rate-hz = 50")).is_ok());
    }

    #[test]
    fn obstacle_servo_and_encoder_settings_become_their_info() {
        let Info::Obstacle(o) = info_of(OBSTACLE, &Settings::new()).unwrap() else { panic!() };
        assert_eq!(o, obstacle::Info { kind: obstacle::Kind::Bumper, trigger_distance_m: Some(0.01) });
        let s = settings("kind = \"infrared\"\ntrigger-distance-m = 0.15\n");
        let Info::Obstacle(o) = info_of(OBSTACLE, &s).unwrap() else { panic!() };
        assert_eq!((o.kind, o.trigger_distance_m), (obstacle::Kind::Infrared, Some(0.15)));
        let Info::Servo(v) = info_of(SERVO, &Settings::new()).unwrap() else { panic!() };
        assert_eq!((v.min_angle_rad, v.safe_state), (0.0, servo::SafeState::Hold), "D29: hold by default");
        assert!((v.max_angle_rad - std::f32::consts::PI).abs() < 1e-6);
        let Info::Servo(v) = info_of(SERVO, &settings("safe-state = \"release\"")).unwrap() else { panic!() };
        assert_eq!(v.safe_state, servo::SafeState::Release);
        let Info::Encoder(e) = info_of(ENCODER, &Settings::new()).unwrap() else { panic!() };
        assert_eq!(e.counts_per_rev, None, "0 means not known");
        let Info::Encoder(e) = info_of(ENCODER, &settings("counts-per-rev = 1496.9")).unwrap() else {
            panic!()
        };
        assert_eq!(e.counts_per_rev, Some(1496.9));
    }

    #[test]
    fn battery_settings_become_its_info_and_its_drain() {
        let s = settings("chemistry = \"lipo\"\ncells-in-series = 4\ncapacity-ah = 5\nidle-current-a = 1\n");
        let Info::Battery(b) = info_of(BATTERY, &s).unwrap() else { panic!() };
        assert_eq!(
            b,
            battery::Info { chemistry: battery::Chemistry::Lipo, cells_in_series: 4, capacity_ah: Some(5.0) }
        );
        assert_eq!(battery_drain(&s), Drain { idle_a: 1.0, moving_a: 3.0, start_charge: 1.0 });
        let Info::Battery(d) = info_of(BATTERY, &Settings::new()).unwrap() else { panic!() };
        assert_eq!(
            (d.chemistry, d.cells_in_series, d.capacity_ah),
            (battery::Chemistry::LiIon, 3, Some(2.2))
        );
    }
}
