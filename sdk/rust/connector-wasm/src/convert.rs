//! Rust SDK types ↔ the WIT types of `faber:connector`, inside the component. The core's
//! `wasm::convert` is the mirror image; the capture test in the core proves the two agree.

use crate::bindings::faber::capabilities as cap;
use crate::bindings::faber::connector::types as wit;
use faber_capabilities::{
    Command, Quaternion, Reading, Twist2d, Vec3, battery, digital_out, distance, estop, motor, planar_base,
    position, pwm_out,
};
use faber_connector::{CommandError, Health, Problem, Settings, SettingsError};

fn twist(t: Twist2d) -> cap::types::Twist2d {
    cap::types::Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn twist_from(t: cap::types::Twist2d) -> Twist2d {
    Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn vec3(v: Vec3) -> cap::types::Vec3 {
    cap::types::Vec3 { x: v.x, y: v.y, z: v.z }
}

fn quaternion(q: Quaternion) -> cap::types::Quaternion {
    cap::types::Quaternion { x: q.x, y: q.y, z: q.z, w: q.w }
}

pub fn reading_to_wit(r: Reading) -> wit::Reading {
    match r {
        Reading::LaserScan(r) => wit::Reading::LaserScan(cap::laser_scan::Reading {
            t_us: r.t_us,
            angle_min: r.angle_min,
            angle_increment: r.angle_increment,
            angles: r.angles,
            time_increment: r.time_increment,
            scan_time: r.scan_time,
            ranges: r.ranges,
            intensities: r.intensities,
        }),
        Reading::Distance(r) => wit::Reading::Distance(cap::distance::Reading {
            t_us: r.t_us,
            meters: r.meters,
            status: match r.status {
                distance::Status::Ok => cap::distance::Status::Ok,
                distance::Status::TooClose => cap::distance::Status::TooClose,
                distance::Status::TooFar => cap::distance::Status::TooFar,
                distance::Status::NoTarget => cap::distance::Status::NoTarget,
                distance::Status::Error => cap::distance::Status::Error,
            },
        }),
        Reading::Motor(r) => wit::Reading::Motor(cap::motor::Reading {
            t_us: r.t_us,
            velocity_rad_s: r.velocity_rad_s,
            position_rad: r.position_rad,
            current_a: r.current_a,
        }),
        Reading::PlanarBase(r) => wit::Reading::PlanarBase(cap::planar_base::Reading {
            t_us: r.t_us,
            commanded: twist(r.commanded),
            measured: r.measured.map(twist),
            bumped: r.bumped,
        }),
        Reading::Estop(r) => wit::Reading::Estop(cap::estop::Reading {
            t_us: r.t_us,
            engaged: r.engaged,
            source: match r.source {
                estop::Source::Part => cap::estop::Source::Part,
                estop::Source::App => cap::estop::Source::App,
                estop::Source::Shell => cap::estop::Source::Shell,
            },
            reason: r.reason,
        }),
        Reading::Pose(r) => wit::Reading::Pose(cap::pose::Reading {
            t_us: r.t_us,
            frame: r.frame,
            position: vec3(r.position),
            orientation: quaternion(r.orientation),
            position_variance: r.position_variance.map(vec3),
            yaw_variance: r.yaw_variance,
        }),
        Reading::Position(r) => wit::Reading::Position(cap::position::Reading {
            t_us: r.t_us,
            latitude_deg: r.latitude_deg,
            longitude_deg: r.longitude_deg,
            altitude_m: r.altitude_m,
            fix: match r.fix {
                position::Fix::None => cap::position::Fix::None,
                position::Fix::Fix2d => cap::position::Fix::Fix2d,
                position::Fix::Fix3d => cap::position::Fix::Fix3d,
                position::Fix::Dgps => cap::position::Fix::Dgps,
                position::Fix::RtkFloat => cap::position::Fix::RtkFloat,
                position::Fix::RtkFixed => cap::position::Fix::RtkFixed,
            },
            satellites: r.satellites,
            horizontal_accuracy_m: r.horizontal_accuracy_m,
            vertical_accuracy_m: r.vertical_accuracy_m,
            speed_m_s: r.speed_m_s,
            course_rad: r.course_rad,
        }),
        Reading::Battery(r) => wit::Reading::Battery(cap::battery::Reading {
            t_us: r.t_us,
            volts: r.volts,
            amps: r.amps,
            charge: r.charge,
            celsius: r.celsius,
            cell_volts: r.cell_volts,
            level: match r.level {
                battery::Level::Ok => cap::battery::Level::Ok,
                battery::Level::Low => cap::battery::Level::Low,
                battery::Level::Critical => cap::battery::Level::Critical,
            },
        }),
        Reading::Temperature(r) => wit::Reading::Temperature(cap::temperature::Reading {
            t_us: r.t_us,
            celsius: r.celsius,
            variance: r.variance,
        }),
        Reading::DigitalIn(r) => {
            wit::Reading::DigitalIn(cap::digital_in::Reading { t_us: r.t_us, level: r.level })
        }
        Reading::DigitalOut(r) => {
            wit::Reading::DigitalOut(cap::digital_out::Reading { t_us: r.t_us, level: r.level })
        }
        Reading::AnalogIn(r) => wit::Reading::AnalogIn(cap::analog_in::Reading {
            t_us: r.t_us,
            fraction: r.fraction,
            volts: r.volts,
        }),
        Reading::PwmOut(r) => wit::Reading::PwmOut(cap::pwm_out::Reading { t_us: r.t_us, duty: r.duty }),
    }
}

fn problem(p: Problem) -> wit::Problem {
    wit::Problem { message: p.message, hint: p.hint }
}

pub fn health_to_wit(h: Health) -> wit::Health {
    match h {
        Health::Starting => wit::Health::Starting,
        Health::Running => wit::Health::Running,
        Health::Degraded(p) => wit::Health::Degraded(problem(p)),
        Health::Failed(p) => wit::Health::Failed(problem(p)),
    }
}

pub fn command_from_wit(c: wit::Command) -> Command {
    match c {
        wit::Command::Motor(m) => Command::Motor(match m {
            cap::motor::Command::SetVelocity(v) => motor::Command::SetVelocity { velocity_rad_s: v },
            cap::motor::Command::SetPosition(p) => motor::Command::SetPosition { position_rad: p },
            cap::motor::Command::SetDuty(d) => motor::Command::SetDuty { duty: d },
        }),
        wit::Command::PlanarBase(cap::planar_base::Command::SetVelocity(t)) => {
            Command::PlanarBase(planar_base::Command::SetVelocity(twist_from(t)))
        }
        wit::Command::DigitalOut(cap::digital_out::Command::Set(level)) => {
            Command::DigitalOut(digital_out::Command::Set { level })
        }
        wit::Command::PwmOut(cap::pwm_out::Command::SetDuty(duty)) => {
            Command::PwmOut(pwm_out::Command::SetDuty { duty })
        }
    }
}

pub fn command_to_wit(c: Command) -> wit::Command {
    match c {
        Command::Motor(m) => wit::Command::Motor(match m {
            motor::Command::SetVelocity { velocity_rad_s } => {
                cap::motor::Command::SetVelocity(velocity_rad_s)
            }
            motor::Command::SetPosition { position_rad } => cap::motor::Command::SetPosition(position_rad),
            motor::Command::SetDuty { duty } => cap::motor::Command::SetDuty(duty),
        }),
        Command::PlanarBase(planar_base::Command::SetVelocity(t)) => {
            wit::Command::PlanarBase(cap::planar_base::Command::SetVelocity(twist(t)))
        }
        Command::DigitalOut(digital_out::Command::Set { level }) => {
            wit::Command::DigitalOut(cap::digital_out::Command::Set(level))
        }
        Command::PwmOut(pwm_out::Command::SetDuty { duty }) => {
            wit::Command::PwmOut(cap::pwm_out::Command::SetDuty(duty))
        }
    }
}

pub fn command_error_to_wit(e: CommandError) -> wit::CommandError {
    match e {
        CommandError::Unsupported => wit::CommandError::Unsupported,
        CommandError::NotReady(p) => wit::CommandError::NotReady(problem(p)),
    }
}

pub fn settings_error_to_wit(e: SettingsError) -> wit::SettingsError {
    wit::SettingsError { key: e.key, message: e.message, hint: e.hint }
}

/// Rebuilds the part's settings table exactly as parts.toml held it.
pub fn settings_from_wit(values: Vec<(String, wit::SettingValue)>) -> Settings {
    values.into_iter().map(|(key, v)| (key, setting_value(v))).collect()
}

fn setting_value(v: wit::SettingValue) -> toml::Value {
    match v {
        wit::SettingValue::Boolean(b) => toml::Value::Boolean(b),
        wit::SettingValue::Integer(i) => toml::Value::Integer(i),
        wit::SettingValue::Float(f) => toml::Value::Float(f),
        wit::SettingValue::Text(s) => toml::Value::String(s),
        wit::SettingValue::TextList(l) => {
            toml::Value::Array(l.into_iter().map(toml::Value::String).collect())
        }
        // TOML text of one value, written by the core; read back through a one-key table.
        wit::SettingValue::Other(text) => format!("v = {text}")
            .parse::<toml::Table>()
            .ok()
            .and_then(|mut t| t.remove("v"))
            .unwrap_or(toml::Value::String(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_capabilities::{analog_in, digital_in, laser_scan, pose, temperature};

    #[test]
    fn settings_come_back_as_parts_toml_held_them() {
        let table: Settings = r#"
            a = true
            b = 3
            c = 0.5
            d = "x"
            e = ["fl", "fr"]
            f = { g = 1 }
            h = 1979-05-27T07:32:00Z
        "#
        .parse()
        .unwrap();
        let values = vec![
            ("a".to_string(), wit::SettingValue::Boolean(true)),
            ("b".to_string(), wit::SettingValue::Integer(3)),
            ("c".to_string(), wit::SettingValue::Float(0.5)),
            ("d".to_string(), wit::SettingValue::Text("x".into())),
            ("e".to_string(), wit::SettingValue::TextList(vec!["fl".into(), "fr".into()])),
            ("f".to_string(), wit::SettingValue::Other(table["f"].to_string())),
            ("h".to_string(), wit::SettingValue::Other(table["h"].to_string())),
        ];
        assert_eq!(settings_from_wit(values), table);
    }

    #[test]
    fn commands_round_trip() {
        for c in [
            Command::Motor(motor::Command::SetVelocity { velocity_rad_s: 1.0 }),
            Command::Motor(motor::Command::SetPosition { position_rad: 2.0 }),
            Command::Motor(motor::Command::SetDuty { duty: -0.5 }),
            Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.1, 0.2, 0.3))),
            Command::DigitalOut(digital_out::Command::Set { level: true }),
            Command::PwmOut(pwm_out::Command::SetDuty { duty: 0.25 }),
        ] {
            assert_eq!(command_from_wit(command_to_wit(c)), c);
        }
    }

    #[test]
    fn every_reading_converts() {
        let readings = [
            Reading::LaserScan(laser_scan::Reading::raw_points(1, 0.1, vec![0.5], vec![f32::INFINITY])),
            Reading::Distance(distance::Reading { t_us: 2, meters: 0.3, status: distance::Status::TooFar }),
            Reading::Motor(motor::Reading { t_us: 3, current_a: Some(0.2), ..Default::default() }),
            Reading::PlanarBase(planar_base::Reading { t_us: 4, bumped: true, ..Default::default() }),
            Reading::Estop(estop::Reading {
                t_us: 5,
                engaged: true,
                source: estop::Source::Shell,
                reason: "r".into(),
            }),
            Reading::Pose(pose::Reading::planar(6, "world", 1.0, 2.0, 0.5)),
            Reading::Position(position::Reading {
                t_us: 7,
                latitude_deg: 55.6761,
                longitude_deg: 12.5683,
                altitude_m: Some(14.0),
                fix: position::Fix::Fix3d,
                satellites: 9,
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                speed_m_s: Some(0.4),
                course_rad: None,
            }),
            Reading::Battery(battery::Reading {
                cell_volts: Some(vec![3.6, 3.61, 3.59]),
                ..battery::Reading::volts(8, 10.8, battery::Level::Low)
            }),
            Reading::Temperature(temperature::Reading { t_us: 9, celsius: -4.5, variance: Some(0.1) }),
            Reading::DigitalIn(digital_in::Reading { t_us: 10, level: true }),
            Reading::DigitalOut(digital_out::Reading { t_us: 11, level: true }),
            Reading::AnalogIn(analog_in::Reading { t_us: 12, fraction: 0.5, volts: None }),
            Reading::PwmOut(pwm_out::Reading { t_us: 13, duty: 0.75 }),
        ];
        let t: Vec<u64> = readings
            .into_iter()
            .map(|r| match reading_to_wit(r) {
                wit::Reading::LaserScan(r) => r.t_us,
                wit::Reading::Distance(r) => r.t_us,
                wit::Reading::Motor(r) => r.t_us,
                wit::Reading::PlanarBase(r) => r.t_us,
                wit::Reading::Estop(r) => r.t_us,
                wit::Reading::Pose(r) => r.t_us,
                wit::Reading::Position(r) => {
                    assert_eq!((r.latitude_deg, r.satellites), (55.6761, 9), "a GPS fix crosses intact");
                    assert!(matches!(r.fix, cap::position::Fix::Fix3d));
                    r.t_us
                }
                wit::Reading::Battery(r) => {
                    assert!(matches!(r.level, cap::battery::Level::Low));
                    assert_eq!(r.cell_volts.as_deref(), Some(&[3.6, 3.61, 3.59][..]));
                    r.t_us
                }
                wit::Reading::Temperature(r) => {
                    assert_eq!((r.celsius, r.variance), (-4.5, Some(0.1)));
                    r.t_us
                }
                wit::Reading::DigitalIn(r) => {
                    assert!(r.level);
                    r.t_us
                }
                wit::Reading::DigitalOut(r) => {
                    assert!(r.level);
                    r.t_us
                }
                wit::Reading::AnalogIn(r) => {
                    assert_eq!((r.fraction, r.volts), (0.5, None));
                    r.t_us
                }
                wit::Reading::PwmOut(r) => {
                    assert_eq!(r.duty, 0.75);
                    r.t_us
                }
            })
            .collect();
        assert_eq!(t, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
    }
}
