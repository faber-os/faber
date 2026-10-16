//! Rust SDK types ↔ the WIT types the app world uses (readings and commands), inside the
//! component. The core's `sandbox::convert` is the mirror image; the wall-follower test in the
//! core proves the two agree.

use crate::bindings::faber::capabilities as cap;
use crate::bindings::faber::connector::types as wit;
use faber_capabilities::{
    Command, Quaternion, Reading, Twist2d, Vec3, battery, distance, estop, laser_scan, motor, planar_base,
    pose, position,
};

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

fn vec3_from(v: cap::types::Vec3) -> Vec3 {
    Vec3 { x: v.x, y: v.y, z: v.z }
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
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_and_commands_round_trip() {
        let readings = [
            Reading::LaserScan(laser_scan::Reading::even_bins(1, -1.0, 0.5, 0.1, vec![1.0, f32::INFINITY])),
            Reading::Distance(distance::Reading { t_us: 2, meters: 0.3, status: distance::Status::TooClose }),
            Reading::PlanarBase(planar_base::Reading {
                t_us: 3,
                commanded: Twist2d::new(0.1, 0.0, 0.2),
                measured: Some(Twist2d::new(0.1, 0.0, 0.1)),
                bumped: true,
            }),
            Reading::Battery(battery::Reading {
                amps: Some(-0.8),
                charge: Some(0.05),
                ..battery::Reading::volts(4, 9.8, battery::Level::Critical)
            }),
        ];
        for r in readings {
            assert_eq!(reading_from_wit(reading_to_wit(r.clone())), r);
        }
        for c in [
            Command::Motor(motor::Command::SetDuty { duty: 0.5 }),
            Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d::new(0.3, 0.0, -0.1))),
        ] {
            assert_eq!(command_from_wit(command_to_wit(c)), c);
        }
    }
}
