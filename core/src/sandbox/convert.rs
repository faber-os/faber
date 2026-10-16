//! The WIT types of `faber:connector` ↔ the Rust types, on the core's side of the sandbox. The
//! mirror image of `faber_connector_wasm`'s and `faber_app_wasm`'s conversions; the LD19 capture
//! test and the wall-follower test prove they agree.

use super::bindings::faber::capabilities as cap;
use super::bindings::faber::connector::types as wit;
use faber_capabilities::{
    Command, Mount, Quaternion, Reading, Twist2d, Vec3, analog_in, battery, digital_in, digital_out,
    distance, encoder, estop, imu, laser_scan, motor, obstacle, planar_base, pose, position, pwm_out, servo,
    temperature,
};
use faber_connector::{CommandError, Health, Problem, Settings};

/// Where a part sits on the machine, as an app's `roles.mount` answers it.
pub fn mount_to_wit(m: Mount) -> cap::types::Mount {
    cap::types::Mount { x: m.x, y: m.y, z: m.z, roll: m.roll, pitch: m.pitch, yaw: m.yaw }
}

fn twist(t: Twist2d) -> cap::types::Twist2d {
    cap::types::Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn twist_from(t: cap::types::Twist2d) -> Twist2d {
    Twist2d { vx: t.vx, vy: t.vy, omega: t.omega }
}

fn vec3_from(v: cap::types::Vec3) -> Vec3 {
    Vec3 { x: v.x, y: v.y, z: v.z }
}

fn quaternion_from(q: cap::types::Quaternion) -> Quaternion {
    Quaternion { x: q.x, y: q.y, z: q.z, w: q.w }
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
            orientation: quaternion_from(r.orientation),
            position_variance: r.position_variance.map(vec3_from),
            yaw_variance: r.yaw_variance,
        }),
        wit::Reading::Position(r) => Reading::Position(position::Reading {
            t_us: r.t_us,
            latitude_deg: r.latitude_deg,
            longitude_deg: r.longitude_deg,
            altitude_m: r.altitude_m,
            fix: match r.fix {
                cap::position::Fix::None => position::Fix::None,
                cap::position::Fix::Fix2d => position::Fix::Fix2d,
                cap::position::Fix::Fix3d => position::Fix::Fix3d,
                cap::position::Fix::Dgps => position::Fix::Dgps,
                cap::position::Fix::RtkFloat => position::Fix::RtkFloat,
                cap::position::Fix::RtkFixed => position::Fix::RtkFixed,
            },
            satellites: r.satellites,
            horizontal_accuracy_m: r.horizontal_accuracy_m,
            vertical_accuracy_m: r.vertical_accuracy_m,
            speed_m_s: r.speed_m_s,
            course_rad: r.course_rad,
        }),
        wit::Reading::Battery(r) => Reading::Battery(battery::Reading {
            t_us: r.t_us,
            volts: r.volts,
            amps: r.amps,
            charge: r.charge,
            celsius: r.celsius,
            cell_volts: r.cell_volts,
            level: match r.level {
                cap::battery::Level::Ok => battery::Level::Ok,
                cap::battery::Level::Low => battery::Level::Low,
                cap::battery::Level::Critical => battery::Level::Critical,
            },
        }),
        wit::Reading::Temperature(r) => Reading::Temperature(temperature::Reading {
            t_us: r.t_us,
            celsius: r.celsius,
            variance: r.variance,
        }),
        wit::Reading::DigitalIn(r) => {
            Reading::DigitalIn(digital_in::Reading { t_us: r.t_us, level: r.level })
        }
        wit::Reading::DigitalOut(r) => {
            Reading::DigitalOut(digital_out::Reading { t_us: r.t_us, level: r.level })
        }
        wit::Reading::AnalogIn(r) => {
            Reading::AnalogIn(analog_in::Reading { t_us: r.t_us, fraction: r.fraction, volts: r.volts })
        }
        wit::Reading::PwmOut(r) => Reading::PwmOut(pwm_out::Reading { t_us: r.t_us, duty: r.duty }),
        wit::Reading::Obstacle(r) => {
            Reading::Obstacle(obstacle::Reading { t_us: r.t_us, detected: r.detected })
        }
        wit::Reading::Servo(r) => Reading::Servo(servo::Reading { t_us: r.t_us, angle_rad: r.angle_rad }),
        wit::Reading::Encoder(r) => Reading::Encoder(encoder::Reading {
            t_us: r.t_us,
            count: r.count,
            delta: r.delta,
            counts_per_s: r.counts_per_s,
            velocity_rad_s: r.velocity_rad_s,
        }),
        wit::Reading::Imu(r) => Reading::Imu(imu::Reading {
            t_us: r.t_us,
            orientation: r.orientation.map(quaternion_from),
            angular_velocity: vec3_from(r.angular_velocity),
            linear_acceleration: vec3_from(r.linear_acceleration),
            orientation_variance: r.orientation_variance.map(vec3_from),
            angular_velocity_variance: r.angular_velocity_variance.map(vec3_from),
            linear_acceleration_variance: r.linear_acceleration_variance.map(vec3_from),
        }),
    }
}

fn vec3(v: Vec3) -> cap::types::Vec3 {
    cap::types::Vec3 { x: v.x, y: v.y, z: v.z }
}

fn quaternion(q: Quaternion) -> cap::types::Quaternion {
    cap::types::Quaternion { x: q.x, y: q.y, z: q.z, w: q.w }
}

/// A reading as the guest receives it (an app reading its roles).
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
        Reading::Obstacle(r) => {
            wit::Reading::Obstacle(cap::obstacle::Reading { t_us: r.t_us, detected: r.detected })
        }
        Reading::Servo(r) => {
            wit::Reading::Servo(cap::servo::Reading { t_us: r.t_us, angle_rad: r.angle_rad })
        }
        Reading::Encoder(r) => wit::Reading::Encoder(cap::encoder::Reading {
            t_us: r.t_us,
            count: r.count,
            delta: r.delta,
            counts_per_s: r.counts_per_s,
            velocity_rad_s: r.velocity_rad_s,
        }),
        Reading::Imu(r) => wit::Reading::Imu(cap::imu::Reading {
            t_us: r.t_us,
            orientation: r.orientation.map(quaternion),
            angular_velocity: vec3(r.angular_velocity),
            linear_acceleration: vec3(r.linear_acceleration),
            orientation_variance: r.orientation_variance.map(vec3),
            angular_velocity_variance: r.angular_velocity_variance.map(vec3),
            linear_acceleration_variance: r.linear_acceleration_variance.map(vec3),
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
        Command::DigitalOut(digital_out::Command::Set { level }) => {
            wit::Command::DigitalOut(cap::digital_out::Command::Set(level))
        }
        Command::PwmOut(pwm_out::Command::SetDuty { duty }) => {
            wit::Command::PwmOut(cap::pwm_out::Command::SetDuty(duty))
        }
        Command::Servo(s) => wit::Command::Servo(match s {
            servo::Command::SetAngle { angle_rad } => cap::servo::Command::SetAngle(angle_rad),
            servo::Command::Release => cap::servo::Command::Release,
            servo::Command::Safe => cap::servo::Command::Safe,
        }),
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
        wit::Command::Servo(s) => Command::Servo(match s {
            cap::servo::Command::SetAngle(a) => servo::Command::SetAngle { angle_rad: a },
            cap::servo::Command::Release => servo::Command::Release,
            cap::servo::Command::Safe => servo::Command::Safe,
        }),
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
            Command::DigitalOut(digital_out::Command::Set { level: false }),
            Command::PwmOut(pwm_out::Command::SetDuty { duty: 0.9 }),
            Command::Servo(servo::Command::SetAngle { angle_rad: 1.25 }),
            Command::Servo(servo::Command::Release),
            Command::Servo(servo::Command::Safe),
        ] {
            assert_eq!(command_from_wit(command_to_wit(c)), c);
        }
    }

    #[test]
    fn readings_round_trip() {
        let readings = [
            Reading::LaserScan(laser_scan::Reading::even_bins(1, -1.0, 0.5, 0.1, vec![1.0, f32::INFINITY])),
            Reading::Distance(distance::Reading { t_us: 2, meters: 0.3, status: distance::Status::TooFar }),
            Reading::Pose(pose::Reading::planar(3, "map", 1.0, 2.0, 0.5)),
            Reading::Estop(estop::Reading {
                t_us: 4,
                engaged: true,
                source: estop::Source::App,
                reason: "x".into(),
            }),
            Reading::Battery(battery::Reading {
                amps: Some(-2.0),
                charge: Some(0.4),
                celsius: Some(31.0),
                cell_volts: Some(vec![3.7, 3.71, 3.69]),
                ..battery::Reading::volts(5, 11.1, battery::Level::Low)
            }),
            Reading::Temperature(temperature::Reading { t_us: 6, celsius: -12.25, variance: Some(0.01) }),
            Reading::DigitalIn(digital_in::Reading { t_us: 7, level: false }),
            Reading::DigitalOut(digital_out::Reading { t_us: 8, level: true }),
            Reading::AnalogIn(analog_in::Reading { t_us: 9, fraction: 1.0, volts: None }),
            Reading::PwmOut(pwm_out::Reading { t_us: 10, duty: 0.0 }),
            Reading::Obstacle(obstacle::Reading { t_us: 11, detected: true }),
            Reading::Servo(servo::Reading { t_us: 12, angle_rad: Some(0.5) }),
            Reading::Servo(servo::Reading { t_us: 13, angle_rad: None }),
            Reading::Encoder(encoder::Reading {
                t_us: 14,
                count: -5_000_000_000,
                delta: -7,
                counts_per_s: -70.0,
                velocity_rad_s: Some(-0.4),
            }),
            Reading::Imu(imu::Reading {
                orientation: Some(Quaternion::from_yaw(0.3)),
                angular_velocity_variance: Some(Vec3 { x: 0.1, y: 0.2, z: 0.3 }),
                ..imu::Reading::raw(15, Vec3 { x: 1.0, y: 2.0, z: 3.0 }, Vec3 { x: 0.0, y: 0.0, z: 9.8 })
            }),
        ];
        for r in readings {
            assert_eq!(reading_from_wit(reading_to_wit(r.clone())), r);
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
