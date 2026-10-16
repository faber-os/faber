//! The Rust session (the core's side) against the C++ library's protocol core (the board's side),
//! over an in-memory pipe with a fake clock. No hardware: what passes here is the same code that
//! runs on the Arduino.

use faber_capabilities::{Command, Info, Reading, battery, distance, estop, motor};
use faberlink::frame;
use faberlink::message::{Applied, Cause, HelloAck, PartInfo, Status, kind};
use faberlink::session::{Event, Session, SessionConfig, SessionError, State};
use faberlink::{Message, frame::Decoder};
use faberlink_interop::FakeBoard;

type Filter = Box<dyn FnMut(Vec<u8>) -> Vec<u8>>;

/// The core and the board, joined by two byte pipes that a test may tamper with.
struct Link {
    session: Session,
    board: FakeBoard,
    now_ms: u32,
    events: Vec<Event>,
    to_board: Filter,
    to_core: Filter,
}

const MOTOR: u8 = 0;
const SENSOR: u8 = 1;

impl Link {
    fn new(heartbeat_timeout_ms: u16) -> Link {
        Link::on(FakeBoard::new(heartbeat_timeout_ms))
    }

    fn on(board: FakeBoard) -> Link {
        Link {
            session: Session::new(SessionConfig::default(), 0),
            board,
            now_ms: 0,
            events: vec![],
            to_board: Box::new(|b| b),
            to_core: Box::new(|b| b),
        }
    }

    /// A link that has finished the handshake.
    fn ready(heartbeat_timeout_ms: u16) -> Link {
        let mut l = Link::new(heartbeat_timeout_ms);
        l.run(20);
        assert_eq!(l.session.state(), State::Ready, "{:?}", l.events);
        l.events.clear();
        l
    }

    /// The parts the board describes, once the handshake is done.
    fn ready_parts(mut self) -> Vec<faberlink::message::PartDescription> {
        self.run(20);
        self.session.parts().to_vec()
    }

    fn now_us(&self) -> u64 {
        self.now_ms as u64 * 1000
    }

    /// Runs both sides for `ms` milliseconds, one millisecond at a time.
    fn run(&mut self, ms: u32) {
        for _ in 0..ms {
            self.now_ms += 1;
            self.session.tick(self.now_us());
            self.flush();
            self.board.tick(self.now_ms);
            self.flush();
        }
    }

    /// Delivers whatever either side has written.
    fn flush(&mut self) {
        let out = (self.to_board)(self.session.take_output());
        self.board.feed(&out, self.now_ms);
        let back = (self.to_core)(self.board.take_output());
        let now = self.now_us();
        self.session.receive(&back, now);
        self.events.extend(self.session.take_events());
    }

    fn command(&mut self, c: Command) {
        self.session.command(MOTOR, c, true).unwrap();
        self.flush();
    }

    fn duty(&mut self, duty: f32) {
        self.command(Command::Motor(motor::Command::SetDuty { duty }));
    }

    fn applied(&mut self) -> Vec<Applied> {
        let mut out = vec![];
        self.events.retain(|e| match e {
            Event::Applied(a) => {
                out.push(*a);
                false
            }
            _ => true,
        });
        out
    }

    fn statuses(&self) -> Vec<Status> {
        self.events.iter().filter_map(|e| if let Event::Status(s) = e { Some(*s) } else { None }).collect()
    }
}

/// Splits a byte stream into frames and keeps those `keep` says yes to.
fn filter_frames(bytes: Vec<u8>, mut keep: impl FnMut(u8) -> bool) -> Vec<u8> {
    let mut out = vec![];
    let mut d = Decoder::new();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == 0 {
            if let Some(Ok(f)) = d.push(b)
                && keep(f.kind)
            {
                out.extend_from_slice(&bytes[start..=i]);
            }
            start = i + 1;
        } else {
            d.push(b);
        }
    }
    out
}

/// The motor's safe state, as the board reports it.
fn stop() -> Command {
    Command::Motor(motor::Command::STOP)
}

/// Runs for 60 ms and returns the last distance reading.
fn last_distance(l: &mut Link, meters: f32) -> distance::Reading {
    l.board.set_distance(meters);
    l.events.clear();
    l.run(60);
    l.events
        .iter()
        .rev()
        .find_map(|e| match e {
            Event::Reading { part: SENSOR, reading: Reading::Distance(r) } => Some(r.clone()),
            _ => None,
        })
        .expect("a distance reading every 50 ms")
}

#[test]
fn handshake_and_describe_give_the_boards_parts() {
    let l = Link::ready(250);
    let b = l.session.board().unwrap();
    assert_eq!(
        (b.version, b.board.as_str(), b.firmware.as_str(), b.firmware_version.as_str()),
        (1, "sim-board", "faberlink-harness", "0.1.0")
    );
    assert_eq!(b.heartbeat_timeout_ms, 250, "the board's timeout, as it reports it");
    assert_eq!(b.max_payload as usize, FakeBoard::max_payload());
    let parts = l.session.parts();
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].id, parts[0].name.as_str(), parts[0].capability.as_str()), (0, "left", "motor@1"));
    assert_eq!(parts[0].lease_ms, 250);
    assert_eq!(
        parts[0].info,
        PartInfo::Known(Info::Motor(motor::Info {
            kind: motor::Kind::Dc,
            modes: motor::Modes { velocity: true, position: false, duty: true },
            max_velocity_rad_s: Some(10.0),
        }))
    );
    assert_eq!(
        (parts[1].id, parts[1].name.as_str(), parts[1].capability.as_str()),
        (1, "front", "distance@1")
    );
    assert_eq!(parts[1].lease_ms, 0, "a sensor has no lease");
    assert_eq!(
        parts[1].info,
        PartInfo::Known(Info::Distance(distance::Info {
            min_m: 0.1,
            max_m: 0.8,
            field_of_view_rad: 0.1,
            kind: distance::Kind::Infrared,
        }))
    );
}

#[test]
fn the_heartbeat_timeout_is_configurable_within_100_to_1000_ms() {
    for (asked, got) in [(150, 150), (50, 100), (5000, 1000)] {
        let l = Link::ready(asked);
        assert_eq!(l.session.board().unwrap().heartbeat_timeout_ms, got);
    }
}

#[test]
fn readings_decode_to_the_right_capability_types() {
    let mut l = Link::ready(250);
    let r = last_distance(&mut l, 0.42);
    assert_eq!((r.meters, r.status), (0.42, distance::Status::Ok));
    assert!(r.t_us > 0 && r.t_us <= l.now_us(), "stamped with the core's clock");
    assert_eq!(
        (last_distance(&mut l, 2.0).meters, last_distance(&mut l, 2.0).status),
        (0.8, distance::Status::TooFar)
    );
    assert_eq!(last_distance(&mut l, 0.01).status, distance::Status::TooClose);
    assert_eq!(last_distance(&mut l, -1.0).status, distance::Status::NoTarget);
    assert!(
        l.events.iter().any(|e| matches!(e, Event::Reading { part: MOTOR, reading: Reading::Motor(m) }
            if m.velocity_rad_s == Some(0.0) && m.position_rad.is_none())),
        "the simulated motor reports its velocity"
    );
    l.events.clear();
    l.run(500);
    let s = l.statuses();
    assert_eq!(s.len(), 5, "a status every 100 ms");
    assert!(!s[0].failsafe && !s[0].estop);
}

#[test]
fn a_battery_reports_its_voltage_and_level() {
    let info =
        battery::Info { chemistry: battery::Chemistry::LiIon, cells_in_series: 3, capacity_ah: Some(15.0) };
    let mut l = Link::on(FakeBoard::with_battery(250, 0, 3, 15.0));
    l.run(20);
    assert_eq!(l.session.state(), State::Ready, "{:?}", l.events);
    let parts = l.session.parts();
    assert_eq!(parts.len(), 3);
    assert_eq!(
        (parts[2].name.as_str(), parts[2].capability.as_str(), parts[2].lease_ms),
        ("battery", "battery@1", 0)
    );
    assert_eq!(parts[2].info, PartInfo::Known(Info::Battery(info.clone())));
    let mut previous = None;
    for (volts, level) in [
        (12.0, battery::Level::Ok),
        (10.4, battery::Level::Low),
        (10.55, battery::Level::Low),
        (10.7, battery::Level::Ok),
        (9.8, battery::Level::Critical),
        (10.0, battery::Level::Critical),
    ] {
        l.board.set_battery_volts(volts);
        l.events.clear();
        l.run(110);
        let r = l
            .events
            .iter()
            .rev()
            .find_map(|e| match e {
                Event::Reading { part: 2, reading: Reading::Battery(r) } => Some(r.clone()),
                _ => None,
            })
            .expect("a battery reading every 100 ms");
        assert_eq!(
            r,
            battery::Reading { t_us: r.t_us, ..battery::Reading::volts(0, volts, level) },
            "{volts} V"
        );
        assert_eq!(info.level(volts, previous), level, "the core's types agree at {volts} V");
        previous = Some(level);
    }
    l.board.set_battery_volts(-1.0);
    l.events.clear();
    l.run(300);
    assert!(
        !l.events.iter().any(|e| matches!(e, Event::Reading { part: 2, .. })),
        "a battery it can't measure sends nothing"
    );
    let unknown = Link::on(FakeBoard::with_battery(250, 2, 4, 0.0)).ready_parts();
    assert_eq!(
        unknown[2].info,
        PartInfo::Known(Info::Battery(battery::Info {
            chemistry: battery::Chemistry::Lifepo4,
            cells_in_series: 4,
            capacity_ah: None
        }))
    );
}

/// The library's thresholds are a copy of the capability types' (it can't link Rust): they must
/// give the same level at every voltage, for every chemistry, from every previous level.
#[test]
fn the_board_and_the_core_agree_on_every_battery_level() {
    for (code, &chemistry) in battery::Chemistry::ALL.iter().enumerate() {
        for cells in [1u8, 3, 4, 6] {
            let board = FakeBoard::with_battery(250, code as u8, cells, 0.0);
            let info = battery::Info { chemistry, cells_in_series: cells, capacity_ah: None };
            let (empty, full) = info.range_volts();
            for &previous in battery::Level::ALL {
                for i in 0..=500 {
                    let v = empty * 0.9 + (full * 1.05 - empty * 0.9) * i as f32 / 500.0;
                    assert_eq!(
                        board.battery_level_at(v, previous as u8),
                        info.level(v, Some(previous)) as u8,
                        "{chemistry:?} {cells}S at {v} V from {previous:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn commands_apply_and_the_board_says_what_it_did() {
    let mut l = Link::ready(250);
    l.duty(0.5);
    assert_eq!(l.board.motor_output(), 0.5);
    let a = l.applied();
    assert_eq!(
        a,
        [Applied {
            part: MOTOR,
            cause: Cause::Command,
            clamped: false,
            command: Command::Motor(motor::Command::SetDuty { duty: 0.5 })
        }]
    );

    l.command(Command::Motor(motor::Command::SetVelocity { velocity_rad_s: -5.0 }));
    assert_eq!(l.board.motor_output(), -0.5, "open loop: 5 of 10 rad/s");

    l.command(Command::Motor(motor::Command::SetVelocity { velocity_rad_s: 25.0 }));
    assert_eq!(l.board.motor_output(), 1.0);
    let a = l.applied();
    assert_eq!(
        a.last().unwrap().command,
        Command::Motor(motor::Command::SetVelocity { velocity_rad_s: 10.0 })
    );
    assert!(a.last().unwrap().clamped, "the board's own limit");

    // A command the motor can't do is refused, and refusing stops it.
    l.command(Command::Motor(motor::Command::SetPosition { position_rad: 1.0 }));
    assert_eq!(l.board.motor_output(), 0.0);
    assert_eq!(l.applied().last().unwrap().cause, Cause::Refused);
    l.command(Command::Motor(motor::Command::SetDuty { duty: f32::NAN }));
    assert_eq!(l.applied().last().unwrap().cause, Cause::Refused);

    l.duty(0.3);
    l.session.safe(Some(MOTOR)).unwrap();
    l.flush();
    assert_eq!(l.board.motor_output(), 0.0);
    assert_eq!(
        l.applied().last().unwrap(),
        &Applied { part: MOTOR, cause: Cause::Safe, clamped: false, command: stop() }
    );
}

#[test]
fn no_heartbeat_stops_the_motor_and_only_a_new_command_starts_it() {
    let mut l = Link::ready(150);
    l.duty(0.6);
    // The Pi freezes: nothing reaches the board any more (the board still talks).
    l.to_board = Box::new(|_| vec![]);
    let frozen_at = l.now_ms;
    let mut stopped_at = None;
    // The motor's own lease is 250 ms and the heartbeat timeout 150 ms, so the heartbeat is
    // what stops it here.
    for _ in 0..200 {
        l.run(1);
        if stopped_at.is_none() && l.board.motor_output() == 0.0 {
            stopped_at = Some(l.now_ms);
        }
    }
    let after = stopped_at.expect("the motor stopped") - frozen_at;
    assert!((100..=151).contains(&after), "stopped {after} ms after the last heartbeat could arrive");
    assert!(l.applied().iter().any(|a| a.cause == Cause::HeartbeatLost && a.command == stop()));
    assert!(l.statuses().iter().any(|s| s.failsafe), "the status says so");

    // The Pi comes back: heartbeats arrive again, but the motor stays stopped.
    l.to_board = Box::new(|b| b);
    let calls = l.board.motor_drive_calls();
    l.events.clear();
    l.run(300);
    assert_eq!(l.board.motor_output(), 0.0, "no auto-resume");
    assert_eq!(l.board.motor_drive_calls(), calls, "the motor was not touched");
    assert!(!l.statuses().last().unwrap().failsafe);

    l.duty(0.4);
    assert_eq!(l.board.motor_output(), 0.4, "a new command starts it");
}

#[test]
fn a_command_without_a_live_heartbeat_is_refused() {
    let mut l = Link::ready(150);
    l.to_board = Box::new(|b| filter_frames(b, |k| k != kind::HEARTBEAT));
    l.run(200);
    l.duty(0.5);
    assert_eq!(l.board.motor_output(), 0.0);
    assert_eq!(l.applied().last().unwrap().cause, Cause::NoHeartbeat);
}

#[test]
fn an_actuator_stops_on_its_own_lease_timeout_even_with_heartbeats() {
    let mut l = Link::ready(250);
    l.duty(0.7);
    let commanded_at = l.now_ms;
    let mut stopped_at = None;
    for _ in 0..400 {
        l.run(1);
        if stopped_at.is_none() && l.board.motor_output() == 0.0 {
            stopped_at = Some(l.now_ms);
        }
    }
    assert_eq!(stopped_at.expect("the motor stopped") - commanded_at, 250);
    assert!(l.applied().iter().any(|a| a.cause == Cause::LeaseExpired));
    assert!(l.statuses().iter().all(|s| !s.failsafe), "heartbeats kept arriving all along");

    // Commands that keep coming keep it running.
    for _ in 0..10 {
        l.duty(0.2);
        l.run(100);
        assert_eq!(l.board.motor_output(), 0.2);
    }
}

#[test]
fn a_hardware_estop_latches_until_reset_on_the_board_and_released_in_faber() {
    let mut l = Link::ready(250);
    l.duty(0.5);
    l.board.press_estop();
    l.flush();
    assert_eq!(l.board.motor_output(), 0.0);
    assert!(
        l.events.iter().any(|e| matches!(e, Event::Estop(x) if x.engaged && x.source == estop::Source::Part))
    );
    assert!(l.applied().iter().any(|a| a.cause == Cause::Estop));

    let refused = |l: &mut Link| {
        l.duty(0.5);
        l.board.motor_output() == 0.0 && l.applied().last().map(|a| a.cause) == Some(Cause::Estop)
    };
    assert!(refused(&mut l), "while latched");
    // Released in Faber first: not enough, the board hasn't been reset.
    l.session.set_estop(false, estop::Source::Shell, "release");
    l.run(10);
    assert!(refused(&mut l), "released in Faber, but not reset on the board");
    // Reset on the board: still not enough, Faber must release after the reset.
    l.board.reset_estop();
    l.run(110);
    assert!(refused(&mut l), "reset on the board, not yet released in Faber");
    let statuses = l.statuses();
    assert!(statuses.last().unwrap().estop && !statuses.last().unwrap().hardware_estop);
    l.session.set_estop(false, estop::Source::Shell, "release");
    l.run(10);
    assert!(!l.board.estop_engaged());
    l.duty(0.5);
    assert_eq!(l.board.motor_output(), 0.5);
}

#[test]
fn faber_engages_the_estop_on_the_board_and_a_lost_estop_frame_is_caught_by_the_heartbeat() {
    let mut l = Link::ready(250);
    l.duty(0.5);
    // The ESTOP frame itself is lost on the wire...
    l.to_board = Box::new(|b| filter_frames(b, |k| k != kind::ESTOP));
    l.session.set_estop(true, estop::Source::Shell, "Stop button");
    l.flush();
    assert_eq!(l.board.motor_output(), 0.5);
    // ...and the next heartbeat (within 50 ms) carries the e-stop anyway.
    l.run(51);
    assert_eq!(l.board.motor_output(), 0.0);
    l.duty(0.5);
    assert_eq!(l.board.motor_output(), 0.0, "refused while engaged");
    l.to_board = Box::new(|b| b);
    l.session.set_estop(false, estop::Source::Shell, "release");
    l.run(60);
    assert_eq!(l.board.motor_output(), 0.0, "releasing doesn't restart anything");
    l.duty(0.5);
    assert_eq!(l.board.motor_output(), 0.5);
}

/// A small fixed-seed generator, so the corruption tests are the same on every run.
struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn corrupt(bytes: Vec<u8>, rng: &mut XorShift) -> Vec<u8> {
    let mut b = bytes;
    if b.is_empty() {
        return b;
    }
    match rng.below(4) {
        0 => {
            let i = rng.below(b.len() * 8);
            b[i / 8] ^= 1 << (i % 8); // a flipped bit
        }
        1 => {
            let keep = rng.below(b.len()); // truncated
            b.truncate(keep);
        }
        2 => {
            let at = rng.below(b.len()); // garbage in the middle
            let junk: Vec<u8> = (0..rng.below(40)).map(|_| rng.next() as u8).collect();
            b.splice(at..at, junk);
        }
        _ => {
            let i = rng.below(b.len()); // a lost byte
            b.remove(i);
        }
    }
    b
}

#[test]
fn a_corrupted_command_is_dropped_never_acted_on() {
    let mut l = Link::ready(250);
    l.duty(0.25);
    let before = l.board.motor_output();
    // Flip each bit of a set-duty 1.0 frame in turn: no flip may reach the motor.
    let wire = Message::Command(faberlink::message::PartCommand {
        part: MOTOR,
        command: Command::Motor(motor::Command::SetDuty { duty: 1.0 }),
    })
    .to_wire(1, 200)
    .unwrap();
    for i in 0..(wire.len() - 1) * 8 {
        let mut bad = wire.clone();
        bad[i / 8] ^= 1 << (i % 8);
        bad.push(0); // ends a frame that lost its delimiter
        l.board.feed(&bad, l.now_ms);
        assert_eq!(l.board.motor_output(), before, "bit {i} reached the motor");
    }
    l.run(150);
    let s = l.statuses();
    assert!(s.last().unwrap().rx_bad > 0, "the board counted the bad frames");
}

#[test]
fn corruption_both_ways_is_dropped_and_the_link_recovers() {
    let mut l = Link::ready(250);
    let mut rng_a = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut rng_b = XorShift(0xD1B5_4A32_D192_ED03);
    // Corrupt one write in five in each direction for two seconds.
    l.to_board = Box::new(move |b| if rng_a.below(5) == 0 { corrupt(b, &mut rng_a) } else { b });
    l.to_core = Box::new(move |b| if rng_b.below(5) == 0 { corrupt(b, &mut rng_b) } else { b });
    for _ in 0..20 {
        let _ = l.session.command(MOTOR, Command::Motor(motor::Command::SetDuty { duty: 0.3 }), true);
        l.run(100);
        let out = l.board.motor_output();
        assert!(out == 0.3 || out == 0.0, "only whole commands are ever applied: {out}");
    }
    let stats = l.session.stats();
    assert!(stats.rx_bad_frames > 0, "{stats:?}");
    // Clean wires again: the link is (or soon is) back to normal.
    l.to_board = Box::new(|b| b);
    l.to_core = Box::new(|b| b);
    l.run(1500);
    assert_eq!(l.session.state(), State::Ready);
    l.events.clear();
    l.duty(0.3);
    assert_eq!(l.board.motor_output(), 0.3);
    l.run(60);
    assert!(l.events.iter().any(|e| matches!(e, Event::Reading { part: SENSOR, .. })));
}

#[test]
fn garbage_on_the_wire_resyncs_at_the_next_zero() {
    let mut l = Link::ready(250);
    let mut rng = XorShift(42);
    for round in 0..200 {
        let junk: Vec<u8> = (0..rng.below(600)).map(|_| rng.next() as u8).collect();
        l.board.feed(&junk, l.now_ms);
        let mut junk_back: Vec<u8> = (0..rng.below(600)).map(|_| rng.next() as u8).collect();
        junk_back.push(0);
        let now = l.now_us();
        l.session.receive(&junk_back, now);
        // The next frame after a 0x00 always gets through.
        l.board.feed(&[0], l.now_ms);
        l.duty(if round % 2 == 0 { 0.1 } else { 0.2 });
        assert_eq!(l.board.motor_output(), if round % 2 == 0 { 0.1 } else { 0.2 }, "round {round}");
    }
}

#[test]
fn a_version_mismatch_gives_a_clear_error() {
    let mut l = Link::new(250);
    l.board.speak_versions(2, 3); // a board with a newer library than this core
    l.run(20);
    assert_eq!(l.session.state(), State::Failed);
    let error = l
        .events
        .iter()
        .find_map(|e| if let Event::Failed(e) = e { Some(e.clone()) } else { None })
        .expect("a Failed event");
    let SessionError::VersionMismatch { core, board, .. } = &error;
    assert_eq!((*core, *board), ((1, 1), (2, 3)));
    assert_eq!(
        error.to_string(),
        "The board \"sim-board\" (faberlink-harness 0.1.0) speaks FaberLink versions 2–3, but this Faber \
         speaks version 1. Update Faber to talk to this board."
    );
    // The board didn't connect either: it sends nothing and applies nothing.
    l.run(300);
    assert!(l.statuses().is_empty());
    l.board.feed(
        &Message::Command(faberlink::message::PartCommand {
            part: MOTOR,
            command: Command::Motor(motor::Command::SetDuty { duty: 1.0 }),
        })
        .to_wire(1, 9)
        .unwrap(),
        l.now_ms,
    );
    assert_eq!(l.board.motor_output(), 0.0);

    // An older board is told to update its firmware instead.
    let older = SessionError::VersionMismatch {
        core: (2, 2),
        board: (1, 1),
        board_name: "mega".into(),
        firmware: "x 1".into(),
    };
    assert!(
        older
            .to_string()
            .ends_with("Update the board's firmware (its FaberLink library) to talk to this Faber.")
    );
}

#[test]
fn the_board_answers_hello_with_its_identity_and_range() {
    // Byte-level: the C++ HELLO_ACK decodes with the Rust messages.
    let mut b = FakeBoard::new(300);
    let hello =
        Message::Hello(faberlink::message::Hello { min_version: 1, max_version: 4, max_payload: 250 });
    b.feed(&hello.to_wire(1, 0).unwrap(), 1);
    let out = b.take_output();
    let mut d = frame::Decoder::new();
    let mut got = vec![];
    for &x in &out {
        if let Some(Ok(f)) = d.push(x) {
            got.push(Message::decode(1, f.kind, f.payload).unwrap());
        }
    }
    assert_eq!(
        got,
        [Message::HelloAck(HelloAck {
            version: 1,
            min_version: 1,
            max_version: 1,
            max_payload: FakeBoard::max_payload() as u8,
            heartbeat_timeout_ms: 300,
            part_count: 2,
            board: "sim-board".into(),
            firmware: "faberlink-harness".into(),
            firmware_version: "0.1.0".into(),
            build_id: None,
        })]
    );
}

/// The HELLO_ACK the board sends after `hello`, decoded with the Rust messages.
fn hello_ack(b: &mut FakeBoard, now_ms: u32) -> HelloAck {
    let hello =
        Message::Hello(faberlink::message::Hello { min_version: 1, max_version: 1, max_payload: 250 });
    b.feed(&hello.to_wire(1, 0).unwrap(), now_ms);
    let mut d = frame::Decoder::new();
    let acks: Vec<HelloAck> = b
        .take_output()
        .iter()
        .filter_map(|&x| {
            d.push(x).and_then(Result::ok).map(|f| Message::decode(1, f.kind, f.payload).unwrap())
        })
        .filter_map(|m| match m {
            Message::HelloAck(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(acks.len(), 1, "{acks:?}");
    acks.into_iter().next().unwrap()
}

#[test]
fn a_sketch_built_by_faber_says_its_build_id_in_hello_ack() {
    let mut b = FakeBoard::new(250).with_build_id(0x8BAD_F00D_0123_4567);
    let ack = hello_ack(&mut b, 1);
    assert_eq!(ack.build_id, Some(0x8BAD_F00D_0123_4567));
    assert_eq!(
        (ack.board.as_str(), ack.firmware_version.as_str()),
        ("sim-board", "0.1.0"),
        "the rest is unchanged"
    );

    // And the session hands it on with the board's identity.
    let mut l = Link::new(250);
    l.board = FakeBoard::mega(250).with_build_id(42);
    l.run(20);
    assert_eq!(l.session.board().unwrap().build_id, Some(42));
    assert_eq!(l.session.state(), State::Ready, "the board connects as before");
}

#[test]
fn a_sketch_built_elsewhere_sends_no_build_id() {
    let mut b = FakeBoard::new(250);
    assert_eq!(hello_ack(&mut b, 1).build_id, None);
    // Every new session says it again.
    let mut b = FakeBoard::new(250).with_build_id(7);
    assert_eq!(hello_ack(&mut b, 1).build_id, Some(7));
    assert_eq!(hello_ack(&mut b, 2).build_id, Some(7));
}

#[test]
fn the_compiled_board_fits_the_documented_budget() {
    // On the PC pointers are 8 bytes (2 on an AVR), so this is an upper bound for the board
    // object with 250-byte buffers: two frame buffers plus a few dozen bytes of state.
    assert_eq!(FakeBoard::max_payload(), 250);
    let size = FakeBoard::sizeof_board();
    assert!(size <= 2 * (250 + 5) + 128, "sizeof(Board) = {size}");
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    /// Random bytes never crash the C++ board, and it still answers the next HELLO.
    #[test]
    fn random_bytes_never_break_the_board(junk in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..2000)) {
        let mut b = FakeBoard::new(250);
        b.feed(&junk, 1);
        b.tick(2);
        b.take_output();
        let hello = Message::Hello(faberlink::message::Hello { min_version: 1, max_version: 1, max_payload: 250 });
        let mut bytes = vec![0];
        bytes.extend(hello.to_wire(1, 0).unwrap());
        b.feed(&bytes, 3);
        let out = b.take_output();
        let mut d = frame::Decoder::new();
        let kinds: Vec<u8> = out.iter().filter_map(|&x| d.push(x).and_then(Result::ok).map(|f| f.kind)).collect();
        proptest::prop_assert!(kinds.contains(&kind::HELLO_ACK), "{:?}", kinds);
    }
}

#[test]
fn the_mega_hosts_two_motors_and_two_distance_sensors_each_on_its_own() {
    use faberlink_interop::{Motor, Sensor};
    let mut l = Link::new(250);
    l.board = FakeBoard::mega(250);
    l.run(20);
    assert_eq!(l.session.board().unwrap().board, "sim-mega");
    let parts: Vec<_> =
        l.session.parts().iter().map(|p| (p.id, p.name.as_str(), p.capability.as_str())).collect();
    assert_eq!(
        parts,
        [
            (0, "left", "motor@1"),
            (1, "right", "motor@1"),
            (2, "front", "distance@1"),
            (3, "rear", "distance@1")
        ]
    );
    l.session.command(1, Command::Motor(motor::Command::SetDuty { duty: 0.5 }), true).unwrap();
    l.flush();
    assert_eq!((l.board.output_of(Motor::Left), l.board.output_of(Motor::Right)), (0.0, 0.5));
    l.board.set_distance_of(Sensor::Rear, 0.3);
    l.events.clear();
    l.run(60);
    let rear = l.events.iter().rev().find_map(|e| match e {
        Event::Reading { part: 3, reading: Reading::Distance(r) } => Some(r.meters),
        _ => None,
    });
    assert_eq!(rear, Some(0.3));
}
