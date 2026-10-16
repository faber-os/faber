//! SENNEP: the Faber app for SENNEP V2, a holonomic (mecanum) robot of 800 × 650 mm with an LD19
//! LiDAR, distance sensors at its corners and a 3S Li-ion pack (examples/apps/sennep/README.md).
//!
//! Its window drives SENNEP by hand; this logic is what actually drives the base, so every
//! command passes its safety layer ([`safety`]) first, whether the window is open or not:
//!
//! - **Driving.** The base is declared `drive = "window"`: the window drives it *through* this
//!   logic (`faber.driveVia("base")`, after the shell's confirm, with the shell's Stop bar), and
//!   sends what the driver asks for as live messages (docs/apps.md §12), [`kinds::DRIVE`] =
//!   `"<x> <y> <turn>"` (each −1..1, a share of the top speed), every 100 ms while a key or the
//!   joystick is held. The logic takes the base's lease and sends the limited velocity on every
//!   tick. When the messages stop for [`DEADMAN_US`], or ask for nothing, or the core says the
//!   window's drive ended (`drive-ended`: the user pressed Stop, the window closed or hung), it
//!   lets go of the lease and the base stops. The core ends the lease itself in those cases too,
//!   and never lets this logic drive the base while no window does.
//! - **Safety layer.** It never drives the leading edge closer than `stop-distance` to anything the
//!   LiDAR or a proximity sensor sees, slowing down on the way, and caps the speed to 30 % on a
//!   critical battery. It says why in the state's `limits`. It only limits this app's own
//!   commands: the core's e-stop and leases are untouched.
//! - **Pose.** `app:sennep/pose` (`pose@1`): dead reckoning from the base's velocity ([`odometry`]).
//!   It drifts. A [`kinds::RESET_POSE`] message puts it back to 0.
//! - **Nearest obstacle.** `app:sennep/nearest-obstacle` (`distance@1`): the gap from the leading
//!   edge to the nearest return in the direction of travel.

pub mod geometry;
pub mod odometry;
pub mod safety;

use faber_app_wasm::{App, Command, DriveError, DriveEvent, Host, LogLevel, Message, Payload, Reading};
use faber_capabilities::{Twist2d, battery, distance, planar_base, pose};
use geometry::{Footprint, Point};
use odometry::Odometry;
use safety::{Blocked, Settings};

/// The window must send `drive` at least this often, or the base stops (µs). Its own timer
/// sends every 100 ms, so this survives two late beats.
pub const DEADMAN_US: u64 = 300_000;
/// Without a new scan for this long, it won't drive (µs): it would be driving blind.
pub const SCAN_STALE_US: u64 = 1_000_000;
/// How far it looks for the nearest obstacle (m): the LD19's range.
pub const LOOK_M: f32 = 12.0;

/// The kinds of the live messages between the window and the logic (docs/apps.md §12). Nothing
/// here is saved.
pub mod kinds {
    /// Window → logic: what the driver asks for, `"<x> <y> <turn>"`.
    pub const DRIVE: &str = "drive";
    /// Window → logic: put the pose back to 0.
    pub const RESET_POSE: &str = "reset-pose";
    /// Window → logic: a window opened; the logic answers it with the limits.
    pub const HELLO: &str = "hello";
    /// Logic → windows: what limits the driving right now (JSON), whenever it changes.
    pub const LIMITS: &str = "limits";
}

/// The role the window drives through the logic.
const BASE: &str = "base";

/// The keys of the app's state: its settings, shared with its window (docs/apps.md §10).
pub mod keys {
    pub const MAX_SPEED: &str = "max-speed";
    pub const MAX_TURN: &str = "max-turn";
    pub const STOP_DISTANCE: &str = "stop-distance";
    /// The footprint (m): SENNEP's 0.8 × 0.65 unless set.
    pub const LENGTH: &str = "length";
    pub const WIDTH: &str = "width";
}

/// What the driver asks for, each −1..1: forward, left, counter-clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stick {
    pub x: f32,
    pub y: f32,
    pub turn: f32,
}

impl Stick {
    /// `"<x> <y> <turn>"`; anything else asks for nothing.
    pub fn parse(text: &str) -> Stick {
        let n: Vec<f32> = text.split_whitespace().filter_map(|w| w.parse().ok()).collect();
        let axis = |v: f32| if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        match n[..] {
            [x, y, turn] => Stick { x: axis(x), y: axis(y), turn: axis(turn) },
            _ => Stick::default(),
        }
    }

    pub fn is_still(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.turn == 0.0
    }
}

fn number(host: &mut dyn Host, key: &str, default: f32, lo: f32, hi: f32) -> f32 {
    host.state_get(key)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
        .clamp(lo, hi)
}

/// The settings in the app's state, each within what makes sense for SENNEP.
pub fn settings(host: &mut dyn Host) -> Settings {
    let d = Settings::default();
    Settings {
        max_speed: number(host, keys::MAX_SPEED, d.max_speed, 0.05, 0.6),
        max_turn: number(host, keys::MAX_TURN, d.max_turn, 0.1, 2.0),
        stop_m: number(host, keys::STOP_DISTANCE, d.stop_m, 0.1, 1.5),
        footprint: Footprint {
            length: number(host, keys::LENGTH, d.footprint.length, 0.1, 3.0),
            width: number(host, keys::WIDTH, d.footprint.width, 0.1, 3.0),
        },
    }
}

#[derive(Default)]
pub struct Sennep {
    odometry: Odometry,
    /// The newest scan's returns (machine frame) and when it came.
    scan: Vec<Point>,
    scan_at_us: Option<u64>,
    proximity: u32,
    /// What the driving window last asked for, and when.
    stick: Option<Stick>,
    stick_at_us: u64,
    /// The window driving the base through this logic.
    driver: Option<u64>,
    /// Windows that said hello this tick: they get the limits.
    hellos: Vec<u64>,
    /// It holds the base's lease.
    driving: bool,
    /// Why it stopped on its own, kept until the next drive.
    stopped_because: Option<String>,
    /// The ways the safety layer blocked last tick.
    blocked: Blocked,
    /// The direction it last went (unit vector), for the nearest obstacle.
    heading: (f32, f32),
    /// The `limits` last sent, so they are only sent when they change.
    limits: String,
}

/// The state's `limits`, as JSON for the window.
fn limits_json(driving: bool, cap: f32, blocked: Blocked, reasons: &[String]) -> String {
    let message: String = reasons.join(" ");
    let mut escaped = String::with_capacity(message.len());
    for c in message.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            c if (c as u32) < 0x20 => escaped.push(' '),
            c => escaped.push(c),
        }
    }
    format!(
        "{{\"driving\":{driving},\"cap\":{cap},\"message\":\"{escaped}\",\"blocked\":{{\"forward\":{},\"back\":{},\"left\":{},\"right\":{},\"turn\":{}}}}}",
        blocked.forward, blocked.back, blocked.left, blocked.right, blocked.turn
    )
}

fn refusal(e: &DriveError) -> String {
    match e {
        DriveError::Held { holder } => format!("Can't drive: {holder} is driving the base."),
        DriveError::EstopEngaged => "Can't drive: the e-stop is engaged. Release it first.".into(),
        DriveError::PartNotReady(m) | DriveError::BadCommand(m) => format!("Can't drive: {m}"),
        DriveError::NoSuchRole => "Can't drive: no part fills the base role.".into(),
        DriveError::LeaseRequired | DriveError::LeaseEnded { .. } => {
            "Can't drive: the base's lease ended. Try again.".into()
        }
    }
}

impl Sennep {
    fn obstacles(&self, host: &mut dyn Host) -> Vec<Point> {
        let mut points = self.scan.clone();
        for i in 0..self.proximity {
            let (Some(Reading::Distance(d)), Some(m)) =
                (host.latest("proximity", i), host.mount("proximity", i))
            else {
                continue;
            };
            points.extend(geometry::proximity_point(&d, m));
        }
        points
    }

    fn battery_critical(host: &mut dyn Host) -> bool {
        matches!(host.latest("battery", 0), Some(Reading::Battery(b)) if b.level == battery::Level::Critical)
    }

    /// One message from a window, or from the core about a window.
    fn heard(&mut self, m: Message, now_us: u64) {
        match m.drive_event() {
            Some(DriveEvent::Ended { role, reason, .. }) if role == BASE && self.driver == Some(m.window) => {
                // The core has ended the lease already; follow nobody until a window drives again.
                self.stick = None;
                self.driver = None;
                self.driving = false;
                self.stopped_because = match reason.as_str() {
                    "stopped" => Some("Stopped: you pressed Stop.".into()),
                    "heartbeat-lost" => Some("Stopped: the window stopped answering.".into()),
                    "window-closed" => Some("Stopped: the window closed.".into()),
                    _ => None,
                };
                return;
            }
            Some(_) => return,
            None => {}
        }
        match m.kind.as_str() {
            kinds::DRIVE => {
                self.stick = m.payload.as_text().map(Stick::parse);
                self.stick_at_us = now_us;
                self.driver = Some(m.window);
            }
            kinds::RESET_POSE => self.odometry.reset(),
            kinds::HELLO => self.hellos.push(m.window),
            _ => {}
        }
    }

    /// The driver's request, if it is fresh and asks for something.
    fn request(&mut self, now_us: u64) -> Option<Stick> {
        let stick = self.stick?;
        if now_us.saturating_sub(self.stick_at_us) > DEADMAN_US {
            // The window stopped sending: closed, hung, or its page went away.
            self.stick = None;
            if !stick.is_still() {
                self.stopped_because =
                    Some("Stopped: the window stopped sending (closed or not responding).".into());
            }
            return None;
        }
        (!stick.is_still()).then_some(stick)
    }

    fn let_go(&mut self, host: &mut dyn Host) {
        if self.driving {
            host.release(BASE, 0);
            self.driving = false;
        }
    }

    fn send(&mut self, host: &mut dyn Host, twist: Twist2d) -> Result<(), DriveError> {
        let go = Command::PlanarBase(planar_base::Command::SetVelocity(twist));
        let sent = match host.send(BASE, 0, go) {
            Err(DriveError::LeaseRequired | DriveError::LeaseEnded { .. }) => {
                host.acquire(BASE, 0).and_then(|()| host.send(BASE, 0, go))
            }
            other => other,
        };
        self.driving = sent.is_ok();
        sent.map(|_| ())
    }
}

impl App for Sennep {
    fn start(&mut self, host: &mut dyn Host, now_us: u64) {
        self.proximity = host.roles().iter().find(|r| r.name == "proximity").map_or(0, |r| r.parts);
        self.heading = (1.0, 0.0);
        self.odometry.step(Twist2d::ZERO, now_us);
        host.log(LogLevel::Info, &format!("started with {} proximity sensor(s)", self.proximity));
    }

    fn tick(&mut self, host: &mut dyn Host, now_us: u64) {
        let s = settings(host);

        // What the windows said since the last tick.
        for m in host.poll_messages() {
            self.heard(m, now_us);
        }

        // Where it is: dead reckoning from the base's own idea of its speed.
        if let Some(Reading::PlanarBase(b)) = host.latest("base", 0) {
            self.odometry.step(b.measured.unwrap_or(b.commanded), now_us);
        }
        let o = self.odometry;
        host.publish("pose", Reading::Pose(pose::Reading::planar(now_us, "odom", o.x, o.y, o.yaw)));

        // What is around.
        if let Some(Reading::LaserScan(scan)) = host.poll("lidar", 0) {
            let mount = host.mount("lidar", 0).unwrap_or_default();
            self.scan = geometry::scan_points(&scan, mount);
            self.scan_at_us = Some(now_us);
        }
        let points = self.obstacles(host);
        let critical = Self::battery_critical(host);
        let seeing = self.scan_at_us.is_some_and(|t| now_us.saturating_sub(t) <= SCAN_STALE_US);

        // What the driver asks, through the safety layer.
        let request = self.request(now_us);
        let asked = request.map_or(Twist2d::ZERO, |k| {
            Twist2d::new(k.x * s.max_speed, k.y * s.max_speed, k.turn * s.max_turn)
        });
        let mut limited = safety::limit(asked, &points, &s, critical, self.blocked);
        self.blocked = limited.blocked;
        if request.is_some() {
            self.stopped_because = None;
            if !seeing {
                limited.twist = Twist2d::ZERO;
                limited.reasons =
                    vec!["Stopped: no LiDAR scan for a second, so it won't drive blind.".into()];
            }
            if let Err(e) = self.send(host, limited.twist) {
                limited.reasons.push(refusal(&e));
            }
        } else {
            self.let_go(host);
            limited.reasons.extend(self.stopped_because.clone());
        }

        // The nearest thing in the direction of travel.
        let speed = asked.vx.hypot(asked.vy);
        if speed > 1e-6 {
            self.heading = (asked.vx / speed, asked.vy / speed);
        }
        let (dx, dy) = self.heading;
        let nearest = safety::gap(&points, s.footprint, dx, dy).filter(|g| *g <= LOOK_M);
        let (meters, status) = match nearest {
            Some(g) => (g, distance::Status::Ok),
            None => (LOOK_M, distance::Status::NoTarget),
        };
        host.publish(
            "nearest-obstacle",
            Reading::Distance(distance::Reading { t_us: now_us, meters, status }),
        );

        // What limits the driving, for the windows: live, never saved.
        let limits = limits_json(self.driving, limited.cap, limited.blocked, &limited.reasons);
        if limits != self.limits {
            let message = limited.reasons.join(" ");
            if !message.is_empty() && !self.limits.contains(&message) {
                host.log(LogLevel::Info, &message);
            }
            if let Err(e) = host.send_message(None, kinds::LIMITS, Payload::Text(limits.clone())) {
                host.log(LogLevel::Warn, &format!("can't tell its windows its limits: {e}"));
            }
            self.limits = limits;
        } else {
            for w in std::mem::take(&mut self.hellos) {
                let _ = host.send_message(Some(w), kinds::LIMITS, Payload::Text(limits.clone()));
            }
        }
        self.hellos.clear();
    }

    fn stop(&mut self, host: &mut dyn Host) {
        self.let_go(host);
    }
}

#[cfg(target_arch = "wasm32")]
faber_app_wasm::export!(Sennep);

#[cfg(test)]
mod tests {
    use super::*;
    use faber_app_wasm::testing::TestHost;
    use faber_app_wasm::{Mount, Role};
    use faber_capabilities::laser_scan;
    use std::f32::consts::PI;

    /// The window driving SENNEP, by its id.
    const W: u64 = 4;

    fn role(name: &str, capability: &str, command: bool, parts: u32) -> Role {
        Role { name: name.into(), capability: capability.into(), command, parts }
    }

    /// SENNEP as the user picked its parts: an LD19 0.2 m ahead of the centre, two proximity
    /// sensors, and a battery. Its base is driven only while a window drives it, and window `W`
    /// does (after the user said yes).
    fn sennep() -> (Sennep, TestHost) {
        let mut host = TestHost::with_roles(vec![
            role("lidar", "laser-scan@1", false, 1),
            role("base", "planar-base@1", true, 1),
            role("proximity", "distance@1", false, 2),
            role("battery", "battery@1", false, 1),
        ]);
        host.window_driven.insert("base".into());
        host.mounts.insert(("lidar".into(), 0), Mount { x: 0.2, ..Mount::default() });
        host.mounts
            .insert(("proximity".into(), 0), Mount { x: -0.4, y: 0.325, yaw: 0.75 * PI, ..Mount::default() });
        host.mounts.insert(
            ("proximity".into(), 1),
            Mount { x: -0.4, y: -0.325, yaw: -0.75 * PI, ..Mount::default() },
        );
        let mut app = Sennep::default();
        app.start(&mut host, 0);
        host.window_drives(W, "base", 0);
        (app, host)
    }

    /// A scan from the LiDAR at x = 0.2 of a wall `gap` ahead of the base's front edge.
    fn wall_ahead(gap: f32, t_us: u64) -> Reading {
        let d = 0.4 + gap - 0.2;
        let n = 360;
        let inc = 2.0 * PI / n as f32;
        let ranges = (0..n)
            .map(|i| {
                let a = -PI + i as f32 * inc;
                if a.cos() > 0.2 { d / a.cos() } else { f32::INFINITY }
            })
            .collect();
        Reading::LaserScan(laser_scan::Reading::even_bins(t_us, -PI, inc, 0.1, ranges))
    }

    fn open_room(t_us: u64) -> Reading {
        Reading::LaserScan(laser_scan::Reading::even_bins(t_us, -PI, 0.1, 0.1, vec![f32::INFINITY; 63]))
    }

    fn last_sent(host: &TestHost) -> Twist2d {
        match host.sent.last() {
            Some((_, _, Command::PlanarBase(planar_base::Command::SetVelocity(t)))) => *t,
            other => panic!("{other:?}"),
        }
    }

    /// What window `W` sends while a control is held.
    fn ask(host: &mut TestHost, x: f32, y: f32, turn: f32) {
        host.window_says(W, kinds::DRIVE, &format!("{x} {y} {turn}"));
    }

    /// The limits the logic last sent its windows.
    fn limits(host: &TestHost) -> String {
        host.outbox
            .iter()
            .rev()
            .find(|(_, k, _)| k == kinds::LIMITS)
            .and_then(|(_, _, p)| p.as_text().map(str::to_string))
            .unwrap_or_default()
    }

    #[test]
    fn the_stick_is_read_from_the_message() {
        assert_eq!(Stick::parse("1 -0.5 0"), Stick { x: 1.0, y: -0.5, turn: 0.0 });
        assert_eq!(Stick::parse("9 0 0").x, 1.0, "never past full");
        assert!(Stick::parse("nonsense").is_still());
        assert!(Stick::parse("NaN 0 0").is_still());
        assert!(Stick::parse("7 1 0 0").is_still(), "the old <seq> form asks for nothing");
    }

    #[test]
    fn it_drives_while_the_window_asks_and_lets_go_when_it_stops() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        app.tick(&mut host, 50_000);
        assert!(host.sent.is_empty() && host.leases.is_empty(), "nobody asked to drive");

        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 100_000);
        assert_eq!(last_sent(&host), Twist2d::new(0.3, 0.0, 0.0), "full stick is the top speed");
        assert!(host.leases.contains(&("base".to_string(), 0)));
        assert!(limits(&host).contains("\"driving\":true"), "{}", limits(&host));

        // The window lets go of the key: it asks for nothing, and the lease goes at once.
        ask(&mut host, 0.0, 0.0, 0.0);
        app.tick(&mut host, 150_000);
        assert!(host.leases.is_empty(), "the base got its safe state");
        assert!(limits(&host).contains("\"driving\":false"));
        assert!(host.state.is_empty(), "driving saves nothing: {:?}", host.state);
    }

    #[test]
    fn a_window_that_stops_sending_stops_the_base() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert_eq!(host.leases.len(), 1);
        // Nothing new 250 ms later: that is fine (it sends every 100 ms)…
        app.tick(&mut host, 300_000);
        assert_eq!(host.leases.len(), 1);
        // …but not past the dead man's time.
        app.tick(&mut host, 400_000);
        assert!(host.leases.is_empty());
        assert!(limits(&host).contains("the window stopped sending"), "{}", limits(&host));
    }

    /// The user pressed the shell's Stop: the core ended the lease and says `drive-ended`. The
    /// app follows that window no more, even though its last message asked to drive.
    #[test]
    fn stop_from_the_shell_ends_the_drive_and_says_why() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        host.window_stops(W, "base", 0, "stopped");
        let sent = host.sent.len();
        app.tick(&mut host, 100_000);
        app.tick(&mut host, 150_000);
        assert_eq!(host.sent.len(), sent, "nothing more is sent");
        assert!(host.leases.is_empty());
        assert!(limits(&host).contains("Stopped: you pressed Stop."), "{}", limits(&host));

        // A window that keeps sending without driving again is refused by the core, in words.
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 200_000);
        assert!(host.leases.is_empty());
        assert!(limits(&host).contains("Can't drive: no window is driving"), "{}", limits(&host));

        // Driving again (after the shell's confirm, which is once per session) works.
        host.window_drives(W, "base", 0);
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 250_000);
        assert_eq!(last_sent(&host).vx, 0.3);
    }

    #[test]
    fn a_hung_window_is_said_in_words() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        host.window_stops(W, "base", 0, "heartbeat-lost");
        app.tick(&mut host, 100_000);
        assert!(limits(&host).contains("Stopped: the window stopped answering."), "{}", limits(&host));
    }

    #[test]
    fn another_windows_drive_ending_doesnt_stop_this_one() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        host.window_says(9, faber_app_wasm::DRIVE_ENDED, "base 0 window-closed");
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 100_000);
        assert_eq!(last_sent(&host).vx, 0.3);
    }

    #[test]
    fn a_new_window_says_hello_and_gets_the_limits() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        app.tick(&mut host, 50_000);
        let first = host.outbox.len();
        assert!(first >= 1, "the first limits go to every window");
        host.window_says(7, kinds::HELLO, "");
        app.tick(&mut host, 100_000);
        let (to, kind, _) = host.outbox.last().unwrap();
        assert_eq!((*to, kind.as_str()), (Some(7), kinds::LIMITS));
        assert_eq!(host.outbox.len(), first + 1);
    }

    #[test]
    fn it_stops_short_of_a_wall_and_says_so() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, wall_ahead(0.3, 1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert_eq!(last_sent(&host).vx, 0.0, "it holds the lease, but doesn't move");
        let l = limits(&host);
        assert!(l.contains("Stopped: something 0.30 m ahead.") && l.contains("\"forward\":true"), "{l}");
        // Backing away is allowed.
        ask(&mut host, -1.0, 0.0, 0.0);
        app.tick(&mut host, 100_000);
        assert_eq!(last_sent(&host).vx, -0.3);
    }

    #[test]
    fn the_stop_distance_comes_from_the_state() {
        let (mut app, mut host) = sennep();
        host.state.insert(keys::STOP_DISTANCE.into(), "0.2".into());
        host.set_reading("lidar", 0, wall_ahead(0.3, 1));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert!((last_sent(&host).vx - 0.15).abs() < 1e-4, "0.1 m left to its stop distance");
        host.state.insert(keys::MAX_SPEED.into(), "0.1".into());
        host.set_reading("lidar", 0, open_room(2));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 100_000);
        assert!((last_sent(&host).vx - 0.1).abs() < 1e-6);
    }

    #[test]
    fn a_proximity_sensor_stops_it_backing_into_something_by_a_corner() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        // The rear-left sensor sees something 0.1 m out, diagonally behind.
        let seen = distance::Reading { t_us: 1, meters: 0.1, status: distance::Status::Ok };
        host.set_reading("proximity", 0, Reading::Distance(seen));
        // Straight back passes beside it…
        ask(&mut host, -1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert_eq!(last_sent(&host).vx, -0.3, "{}", limits(&host));
        // …but backing towards the left would hit it.
        ask(&mut host, -0.7, 0.7, 0.0);
        app.tick(&mut host, 100_000);
        let t = last_sent(&host);
        assert_eq!((t.vx, t.vy), (0.0, 0.0), "{}", limits(&host));
        assert!(limits(&host).contains("Stopped: something 0.10 m in the way."), "{}", limits(&host));
    }

    #[test]
    fn a_critical_battery_caps_the_speed() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        let info =
            battery::Info { chemistry: battery::Chemistry::LiIon, cells_in_series: 3, capacity_ah: None };
        host.set_reading("battery", 0, Reading::Battery(info.reading(1, 9.6, None)));
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert!((last_sent(&host).vx - 0.09).abs() < 1e-6);
        assert!(limits(&host).contains("Battery critical: speed capped to 30 %."));
    }

    #[test]
    fn it_wont_drive_blind() {
        let (mut app, mut host) = sennep();
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert_eq!(last_sent(&host), Twist2d::ZERO);
        assert!(limits(&host).contains("no LiDAR scan"), "{}", limits(&host));
    }

    #[test]
    fn someone_else_driving_is_said_in_words() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        host.refuse = Some(DriveError::Held { holder: "Drive window".into() });
        ask(&mut host, 1.0, 0.0, 0.0);
        app.tick(&mut host, 50_000);
        assert!(
            limits(&host).contains("Can't drive: Drive window is driving the base."),
            "{}",
            limits(&host)
        );
    }

    #[test]
    fn it_publishes_its_pose_and_the_nearest_obstacle_ahead() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, wall_ahead(1.0, 1));
        let moving = planar_base::Reading {
            t_us: 1,
            commanded: Twist2d::new(0.5, 0.0, 0.0),
            measured: Some(Twist2d::new(0.2, 0.0, 0.0)),
            bumped: false,
        };
        host.set_reading("base", 0, Reading::PlanarBase(moving));
        for i in 1..=20 {
            app.tick(&mut host, i * 50_000);
        }
        let pose = host.published.iter().rev().find_map(|(n, r)| match (n.as_str(), r) {
            ("pose", Reading::Pose(p)) => Some(p.clone()),
            _ => None,
        });
        let pose = pose.unwrap();
        assert_eq!(pose.frame, "odom");
        assert!((pose.position.x - 0.2).abs() < 0.011, "measured, not commanded: {pose:?}");
        let nearest = host.published.iter().rev().find_map(|(n, r)| match (n.as_str(), r) {
            ("nearest-obstacle", Reading::Distance(d)) => Some(d.clone()),
            _ => None,
        });
        let nearest = nearest.unwrap();
        assert_eq!(nearest.status, distance::Status::Ok);
        assert!((nearest.meters - 1.0).abs() < 0.02, "{nearest:?}");

        host.window_says(W, kinds::RESET_POSE, "");
        app.tick(&mut host, 21 * 50_000);
        let Some((_, Reading::Pose(p))) = host.published.iter().rev().find(|(n, _)| n == "pose") else {
            panic!()
        };
        assert!(p.position.x.abs() < 0.011, "reset: {p:?}");
    }

    #[test]
    fn stopping_the_app_lets_go_of_the_base() {
        let (mut app, mut host) = sennep();
        host.set_reading("lidar", 0, open_room(1));
        ask(&mut host, 0.0, 1.0, 0.0);
        app.tick(&mut host, 50_000);
        assert_eq!(last_sent(&host), Twist2d::new(0.0, 0.3, 0.0), "it slides left");
        app.stop(&mut host);
        assert!(host.leases.is_empty());
    }

    #[test]
    fn the_limits_are_json_the_window_can_read() {
        let blocked = Blocked { forward: true, ..Blocked::default() };
        let j = limits_json(true, 0.3, blocked, &["Can't drive: \"x\\y\" is driving.".to_string()]);
        assert_eq!(
            j,
            r#"{"driving":true,"cap":0.3,"message":"Can't drive: \"x\\y\" is driving.","blocked":{"forward":true,"back":false,"left":false,"right":false,"turn":false}}"#
        );
    }
}
