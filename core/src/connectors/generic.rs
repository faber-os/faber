//! Generic connectors (docs/simulation.md, section 3): a motor, a drive base and a distance
//! sensor described by their settings alone. Real ones are reached through a FaberLink bridge
//! (Phase 4), so for now they can only be simulated; their manifests say so with
//! [`PortRequirement::Bridge`].
//!
//! The settings are the part's limits, which become its capability info: the core clamps every
//! command to them, and simulations respect them.

use faber_capabilities::{Info, Twist2d, distance, motor, planar_base};
use faber_connector::{
    Connector, Context, Definition, Manifest, PortRequirement, Provide, Settings, SettingsError,
};

pub const PLANAR_BASE: &str = "generic-planar-base";
pub const MOTOR: &str = "generic-motor";
pub const DISTANCE: &str = "generic-distance";

pub fn definitions() -> Vec<Definition> {
    vec![
        definition(PLANAR_BASE, "Generic drive base", base_provides),
        definition(MOTOR, "Generic motor", motor_provides),
        definition(DISTANCE, "Generic distance sensor", distance_provides),
    ]
}

fn definition(
    id: &str,
    name: &str,
    provides: fn(&Manifest, &Settings) -> Result<Vec<Provide>, SettingsError>,
) -> Definition {
    let mut manifest = Manifest {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: PortRequirement::Bridge,
        provides: Vec::new(),
        usb_hints: Vec::new(),
    };
    manifest.provides = provides(&manifest, &Settings::new()).expect("the defaults are valid");
    let create = match id {
        PLANAR_BASE => create_base,
        MOTOR => create_motor,
        _ => create_distance,
    };
    Definition { manifest, create, provides }
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

fn base_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::PlanarBase(base_info(s)?))])
}

fn motor_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Motor(motor_info(s)?))])
}

fn distance_provides(_: &Manifest, s: &Settings) -> Result<Vec<Provide>, SettingsError> {
    Ok(vec![Provide::with_info(Info::Distance(distance_info(s)?))])
}

/// A drive base's `length` × `width` (m) when its settings don't say.
pub const DEFAULT_FOOTPRINT: (f64, f64) = (0.5, 0.4);

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

/// Settings every simulated part may have: how often it publishes.
const COMMON: &[&str] = &["rate-hz"];

fn base_info(s: &Settings) -> Result<planar_base::Info, SettingsError> {
    let r = Reader::new(
        s,
        "drive base",
        &["holonomic", "max-vx", "max-vy", "max-omega", "max-accel", "length", "width", "built-from"],
    )?;
    let max =
        Twist2d::new(r.positive("max-vx", 0.5)?, r.positive("max-vy", 0.5)?, r.positive("max-omega", 1.5)?);
    let accel = r.positive("max-accel", 1.0)?;
    r.positive("length", DEFAULT_FOOTPRINT.0 as f32)?;
    r.positive("width", DEFAULT_FOOTPRINT.1 as f32)?;
    r.positive("rate-hz", 20.0)?;
    // One acceleration setting: turning takes as long to reach full speed as driving does.
    let omega_accel = accel * max.omega / max.vx.max(max.vy);
    Ok(planar_base::Info {
        holonomic: r.bool("holonomic", true)?,
        max_velocity: max,
        max_acceleration: Twist2d::new(accel, accel, omega_accel),
        built_from: r.names("built-from")?,
    })
}

fn motor_info(s: &Settings) -> Result<motor::Info, SettingsError> {
    let r = Reader::new(s, "motor", &["max-velocity", "kind"])?;
    r.positive("rate-hz", 20.0)?;
    let kinds: Vec<&str> = motor::Kind::ALL.iter().map(|k| k.id()).collect();
    let kind = r.choice("kind", "dc", &kinds)?;
    Ok(motor::Info {
        kind: motor::Kind::ALL.iter().copied().find(|k| k.id() == kind).expect("checked by choice"),
        modes: motor::Modes { velocity: true, position: false, duty: true },
        max_velocity_rad_s: Some(r.positive("max-velocity", 10.0)?),
    })
}

fn distance_info(s: &Settings) -> Result<distance::Info, SettingsError> {
    let r = Reader::new(s, "distance sensor", &["min-m", "max-m", "fov-rad", "kind"])?;
    r.positive("rate-hz", 10.0)?;
    let kinds: Vec<&str> = distance::Kind::ALL.iter().map(|k| k.id()).collect();
    let kind = r.choice("kind", "ultrasonic", &kinds)?;
    let (min_m, max_m) = (r.number("min-m", 0.02)?, r.positive("max-m", 4.0)?);
    if !(0.0..max_m).contains(&min_m) {
        return Err(SettingsError {
            key: "min-m".into(),
            message: format!("min-m ({min_m} m) must be at least 0 and less than max-m ({max_m} m)."),
            hint: "Use the range from the sensor's datasheet, e.g. min-m = 0.02 and max-m = 4.0.".into(),
        });
    }
    Ok(distance::Info {
        min_m,
        max_m,
        field_of_view_rad: r.positive("fov-rad", 0.26)?,
        kind: distance::Kind::ALL.iter().copied().find(|k| k.id() == kind).expect("checked by choice"),
    })
}

/// Reads settings with defaults, and explains every mistake.
struct Reader<'a> {
    s: &'a Settings,
}

impl<'a> Reader<'a> {
    /// Refuses keys that are neither in `known` nor in [`COMMON`].
    fn new(s: &'a Settings, what: &str, known: &[&str]) -> Result<Reader<'a>, SettingsError> {
        if let Some(key) = s.keys().find(|k| !known.contains(&k.as_str()) && !COMMON.contains(&k.as_str())) {
            let mut all: Vec<&str> = known.iter().chain(COMMON).copied().collect();
            all.sort_unstable();
            return Err(SettingsError {
                key: key.clone(),
                message: format!("A generic {what} has no setting called \"{key}\"."),
                hint: format!("Its settings are {}. Check the spelling.", all.join(", ")),
            });
        }
        Ok(Reader { s })
    }

    fn number(&self, key: &str, default: f32) -> Result<f32, SettingsError> {
        match self.s.get(key) {
            None => Ok(default),
            Some(v) => match v.as_float().or_else(|| v.as_integer().map(|i| i as f64)) {
                Some(n) if n.is_finite() => Ok(n as f32),
                _ => Err(SettingsError {
                    key: key.into(),
                    message: format!("{key} must be a number, not {v}."),
                    hint: format!("For example {key} = {default}."),
                }),
            },
        }
    }

    fn positive(&self, key: &str, default: f32) -> Result<f32, SettingsError> {
        let n = self.number(key, default)?;
        if n > 0.0 {
            return Ok(n);
        }
        Err(SettingsError {
            key: key.into(),
            message: format!("{key} must be more than 0, not {n}."),
            hint: format!("For example {key} = {default}."),
        })
    }

    fn bool(&self, key: &str, default: bool) -> Result<bool, SettingsError> {
        match self.s.get(key) {
            None => Ok(default),
            Some(v) => v.as_bool().ok_or_else(|| SettingsError {
                key: key.into(),
                message: format!("{key} must be true or false, not {v}."),
                hint: format!("For example {key} = {default}."),
            }),
        }
    }

    fn choice(&self, key: &str, default: &'a str, options: &[&'a str]) -> Result<&'a str, SettingsError> {
        let Some(v) = self.s.get(key) else { return Ok(default) };
        v.as_str().and_then(|s| options.iter().copied().find(|o| *o == s)).ok_or_else(|| SettingsError {
            key: key.into(),
            message: format!("{key} = {v} is not one of the choices."),
            hint: format!(
                "Use one of {}.",
                options.iter().map(|o| format!("\"{o}\"")).collect::<Vec<_>>().join(", ")
            ),
        })
    }

    /// A list of part names, e.g. `built-from = ["fl", "fr"]`.
    fn names(&self, key: &str) -> Result<Vec<String>, SettingsError> {
        let Some(v) = self.s.get(key) else { return Ok(Vec::new()) };
        let bad = || SettingsError {
            key: key.into(),
            message: format!("{key} must be a list of part names, not {v}."),
            hint: format!("For example {key} = [\"front-left\", \"front-right\"]."),
        };
        v.as_array().ok_or_else(bad)?.iter().map(|n| n.as_str().map(str::to_string).ok_or_else(bad)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::Capability;

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
            assert!(d.manifest.provides[0].info.is_some(), "{}", d.manifest.id);
        }
        let caps: Vec<_> = definitions().iter().map(|d| d.manifest.provides[0].capability).collect();
        assert_eq!(caps, [Capability::PlanarBase, Capability::Motor, Capability::Distance]);
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
    fn every_generic_part_accepts_a_rate() {
        for id in [PLANAR_BASE, MOTOR, DISTANCE] {
            assert!(info_of(id, &settings("rate-hz = 5")).is_ok(), "{id}");
        }
    }
}
