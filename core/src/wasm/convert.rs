//! The WIT types of `faber:connector` ↔ the Rust types, on the core's side of the sandbox. The
//! mirror image of `faber_connector_wasm`'s conversions; the LD19 capture test proves they agree.

use super::bindings::faber::capabilities as cap;
use super::bindings::faber::connector::types as wit;
use faber_capabilities::{
    Command, Quaternion, Reading, Twist2d, Vec3, distance, estop, laser_scan, motor, planar_base, pose,
};
use faber_connector::{CommandError, Health, Problem, Settings};

fn twist(t: Twist2d) -> cap::types::Twist2d {
    cap::types::Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn twist_from(t: cap::types::Twist2d) -> Twist2d {
    Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn vec3_from(v: cap::types::Vec3) -> Vec3 {
    Vec3 { x: v.x, y: v.y, z: v.z }
}

pub fn reading_from_wit(r: wit::Reading) -> Reading {
    match r {
        wit::Reading::LaserScan(r) => Reading::LaserScan(laser_scan::Reading {
            t_us: r.t_us,
            angle_min: r.angle_min,
            angle_increment: r.angle_increment,
            angles: r.angles,
            time_increment: r.time_increment,
            scan_time: r.scan_time,
            ranges: r.ranges,
            intensities: r.intensities,
        }),
        wit::Reading::Distance(r) => Reading::Distance(distance::Reading {
            t_us: r.t_us,
            meters: r.meters,
            status: match r.status {
                cap::distance::Status::Ok => distance::Status::Ok,
                cap::distance::Status::TooClose => distance::Status::TooClose,
                cap::distance::Status::TooFar => distance::Status::TooFar,
                cap::distance::Status::NoTarget => distance::Status::NoTarget,
                cap::distance::Status::Error => distance::Status::Error,
            },
        }),
        wit::Reading::Motor(r) => Reading::Motor(motor::Reading {
            t_us: r.t_us,
            velocity_rad_s: r.velocity_rad_s,
            position_rad: r.position_rad,
            current_a: r.current_a,
        }),
        wit::Reading::PlanarBase(r) => Reading::PlanarBase(planar_base::Reading {
            t_us: r.t_us,
            commanded: twist_from(r.commanded),
            measured: r.measured.map(twist_from),
            bumped: r.bumped,
        }),
        wit::Reading::Estop(r) => Reading::Estop(estop::Reading {
            t_us: r.t_us,
            engaged: r.engaged,
            source: match r.source {
                cap::estop::Source::Part => estop::Source::Part,
                cap::estop::Source::App => estop::Source::App,
                cap::estop::Source::Shell => estop::Source::Shell,
            },
            reason: r.reason,
        }),
        wit::Reading::Pose(r) => Reading::Pose(pose::Reading {
            t_us: r.t_us,
            frame: r.frame,
            position: vec3_from(r.position),
            orientation: Quaternion {
                x: r.orientation.x,
                y: r.orientation.y,
                z: r.orientation.z,
                w: r.orientation.w,
            },
            position_variance: r.position_variance.map(vec3_from),
            yaw_variance: r.yaw_variance,
        }),
    }
}

fn problem(p: wit::Problem) -> Problem {
    Problem { message: p.message, hint: p.hint }
}

pub fn health_from_wit(h: wit::Health) -> Health {
    match h {
        wit::Health::Starting => Health::Starting,
        wit::Health::Running => Health::Running,
        wit::Health::Degraded(p) => Health::Degraded(problem(p)),
        wit::Health::Failed(p) => Health::Failed(problem(p)),
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
    }
}

pub fn command_error_from_wit(e: wit::CommandError) -> CommandError {
    match e {
        wit::CommandError::Unsupported => CommandError::Unsupported,
        wit::CommandError::NotReady(p) => CommandError::NotReady(problem(p)),
    }
}

/// The part's settings table, value by value. Anything WIT has no case for travels as TOML text,
/// so the connector sees exactly what parts.toml holds.
pub fn settings_to_wit(settings: &Settings) -> Vec<(String, wit::SettingValue)> {
    settings
        .iter()
        .map(|(key, v)| {
            let value = match v {
                toml::Value::Boolean(b) => wit::SettingValue::Boolean(*b),
                toml::Value::Integer(i) => wit::SettingValue::Integer(*i),
                toml::Value::Float(f) => wit::SettingValue::Float(*f),
                toml::Value::String(s) => wit::SettingValue::Text(s.clone()),
                toml::Value::Array(items) if items.iter().all(toml::Value::is_str) => {
                    wit::SettingValue::TextList(
                        items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
                    )
                }
                other => wit::SettingValue::Other(other.to_string()),
            };
            (key.clone(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_round_trip() {
        for c in [
            Command::Motor(motor::Command::SetVelocity { velocity_rad_s: 1.0 }),
            Command::Motor(motor::Command::SetPosition { position_rad: 2.0 }),
            Command::Motor(motor::Command::SetDuty { duty: -0.5 }),
            Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.1, 0.2, 0.3))),
        ] {
            assert_eq!(command_from_wit(command_to_wit(c)), c);
        }
    }

    #[test]
    fn settings_keep_their_types() {
        let s: Settings = "a = true\nb = 3\nc = 0.5\nd = \"x\"\ne = [\"fl\"]\nf = [1, 2]\n".parse().unwrap();
        let v = settings_to_wit(&s);
        assert!(matches!(v[0].1, wit::SettingValue::Boolean(true)));
        assert!(matches!(v[1].1, wit::SettingValue::Integer(3)));
        assert!(matches!(v[2].1, wit::SettingValue::Float(f) if f == 0.5));
        assert!(matches!(&v[3].1, wit::SettingValue::Text(t) if t == "x"));
        assert!(matches!(&v[4].1, wit::SettingValue::TextList(l) if l == &["fl"]));
        assert!(matches!(&v[5].1, wit::SettingValue::Other(t) if t == "[1, 2]"), "{:?}", v[5].1);
    }

    #[test]
    fn health_keeps_its_problem() {
        let p = wit::Problem { message: "m".into(), hint: "h".into() };
        assert_eq!(health_from_wit(wit::Health::Degraded(p)), Health::Degraded(Problem::new("m", "h")));
    }
}
