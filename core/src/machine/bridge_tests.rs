//! The machine with FaberLink boards: sub-parts, their leases and commands, the e-stop both ways,
//! and checking a port for a board. The boards sit behind a fake serial port.

use super::*;
use crate::bridge::testboard::{Board, ScriptedBoard, Wire, motor_info};
use crate::clock::MonotonicClock;
use crate::ports::Discovered;
use faber_capabilities::{Command, Twist2d, motor, planar_base};
use faberlink::Message;
use faberlink_interop::{FakeBoard, Motor};
use std::time::Instant;

/// The board's port, which can be pulled out (gone from the system's list) and put back.
#[derive(Default)]
struct OnePort {
    gone: AtomicBool,
}

impl SerialDiscovery for OnePort {
    fn list(&self) -> Vec<Discovered> {
        if self.gone.load(Ordering::Relaxed) {
            return Vec::new();
        }
        vec![Discovered { path: "FAKE".into(), usb: None, bluetooth: false }]
    }
}

struct NoDisks;

impl DiskDiscovery for NoDisks {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

const PORT: &str = "serial:FAKE";

fn machine_with<B: Board>(dir: &Path, board: B) -> (Arc<Machine>, Wire<B>) {
    let (m, wire, _port) = machine_on(dir, board);
    (m, wire)
}

fn machine_on<B: Board>(dir: &Path, board: B) -> (Arc<Machine>, Wire<B>, Arc<OnePort>) {
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let wire = Wire::new(board, Arc::clone(&clock));
    let port = Arc::new(OnePort::default());
    let m = Machine::start(MachineOptions {
        data_dir: dir.to_path_buf(),
        clock,
        discovery: Arc::clone(&port) as Arc<dyn SerialDiscovery>,
        usb_problems: Arc::new(NoUsbProblems),
        disks: Arc::new(NoDisks),
        opener: Arc::new(wire.clone()),
        hardware: Arc::new(crate::ports::NoHardware),
        replays: ReplayLibrary::default(),
        connectors: crate::connectors::built_in(),
        timing: Timing {
            backoff_start: Duration::from_millis(10),
            port_retry_max: Duration::from_millis(50),
            ..Timing::default()
        },
        port_scan: None,
        world: Arc::new(simparts::SimWorld::new()),
    })
    .unwrap();
    (m, wire, port)
}

struct NoUsbProblems;

impl UsbProblemDiscovery for NoUsbProblems {
    fn list(&self) -> Vec<crate::ports::FailedUsb> {
        Vec::new()
    }
}

fn add_bridge(m: &Machine) -> PartView {
    m.add_part(NewPart {
        address: None,
        name: "mega".into(),
        connector: BRIDGE_CONNECTOR.into(),
        port: Some(PORT.into()),
        simulate: None,
        mount: Mount::default(),
        settings: Settings::new(),
    })
    .unwrap()
}

fn mega(dir: &Path) -> (Arc<Machine>, Wire<FakeBoard>) {
    let (m, wire) = machine_with(dir, FakeBoard::mega(250));
    add_bridge(&m);
    status(&m, "mega.left", |s| *s == PartStatus::Running);
    (m, wire)
}

fn view(m: &Machine, name: &str) -> Option<PartView> {
    m.parts().into_iter().find(|p| p.name.as_str() == name)
}

/// Waits until the part's status matches, or fails after 3 s.
fn status(m: &Machine, name: &str, f: impl Fn(&PartStatus) -> bool) -> PartStatus {
    let start = Instant::now();
    loop {
        let s = view(m, name).map(|p| p.status);
        if let Some(s) = &s
            && f(s)
        {
            return s.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(3), "timed out; {name} is {s:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn eventually(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(3), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn duty(d: f32) -> Command {
    Command::Motor(motor::Command::SetDuty { duty: d })
}

fn left(wire: &Wire<FakeBoard>) -> f32 {
    wire.board(|b| b.output_of(Motor::Left))
}

#[test]
fn the_bridge_and_the_board_agree_on_the_connector_id() {
    assert_eq!(BRIDGE_CONNECTOR, crate::connectors::bridge::ID);
}

#[test]
fn a_board_on_its_port_shows_its_parts_as_sub_parts_under_the_bridge() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _wire) = mega(dir.path());
    let names: Vec<_> = m.parts().into_iter().map(|p| p.name.to_string()).collect();
    assert_eq!(names, ["mega", "mega.left", "mega.right", "mega.front", "mega.rear"]);
    let bridge = view(&m, "mega").unwrap();
    assert_eq!(bridge.status, PartStatus::Running);
    let board = bridge.board.unwrap();
    assert_eq!(board.link, bridge::LinkState::Ready);
    assert_eq!(board.board.unwrap().board, "sim-mega");
    assert_eq!(bridge.signals[0].address.to_string(), "mega/estop");
    let left = view(&m, "mega.left").unwrap();
    assert_eq!(left.bridge.as_ref().map(PartName::as_str), Some("mega"));
    assert_eq!(left.signals[0].address.to_string(), "mega.left/motor");
    assert!(matches!(left.signals[0].info, Some(Info::Motor(_))));
    assert_eq!(left.source, Source::Port(PORT.into()));
    let text = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
    assert!(text.contains("[parts.mega]") && !text.contains("left"), "the board says what it has:\n{text}");
    assert_eq!(m.ports()[0].claimed_by.as_deref(), Some("mega"));
}

#[test]
fn parts_toml_places_board_parts_and_names_missing_ones() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::write(
        dir.path().join("config/parts.toml"),
        "[parts.mega]\nconnector = \"faber-bridge\"\nport = \"serial:FAKE\"\n\n[parts.mega.parts.left]\nmount = { x = 0.3 }\n\n[parts.mega.parts.arm] # an old part\n",
    )
    .unwrap();
    let (m, _wire) = machine_with(dir.path(), FakeBoard::mega(250));
    status(&m, "mega.left", |s| *s == PartStatus::Running);
    assert_eq!(view(&m, "mega.left").unwrap().mount.x, 0.3);
    let PartStatus::Degraded(p) = status(&m, "mega.arm", |s| matches!(s, PartStatus::Degraded(_))) else {
        unreachable!()
    };
    assert!(p.message.starts_with("Not on the board"), "{p:?}");
    assert!(p.hint.contains("[parts.mega.parts.arm]"), "{p:?}");
    // The stale entry can be removed; a part the board has can't.
    m.remove_part("mega.arm").unwrap();
    assert!(view(&m, "mega.arm").is_none());
    let text = std::fs::read_to_string(dir.path().join("config/parts.toml")).unwrap();
    assert!(!text.contains("arm") && text.contains("[parts.mega.parts.left]"), "{text}");
    let e = m.remove_part("mega.left").unwrap_err();
    assert_eq!((e.kind, e.code), (ErrorKind::Conflict, "part-on-board"));
    assert_eq!(m.remove_part("mega.nope").unwrap_err().code, "unknown-part");
}

#[test]
fn a_sub_parts_lease_gates_its_commands_and_its_end_stops_the_motor() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let e = m.command("mega.left", "not-a-lease", duty(0.5)).unwrap_err();
    assert_eq!(e.code, "lease-required");
    assert_eq!(left(&wire), 0.0, "nothing reaches the board without a lease");
    assert_eq!(m.request_lease("mega.front", "x", false).unwrap_err().code, "not-leasable");
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    let reply = m.command("mega.left", &lease.id, duty(0.5)).unwrap();
    assert_eq!(reply.applied, duty(0.5));
    assert_eq!(left(&wire), 0.5, "the board's motor runs");
    m.release_lease("mega.left", &lease.id).unwrap();
    assert_eq!(left(&wire), 0.0, "SAFE on release");
    let sent = wire.sent();
    assert!(sent.iter().any(|m| matches!(m, Message::Safe(s) if s.part == Some(0))), "{sent:?}");
}

#[test]
fn a_lease_nobody_renews_expires_and_the_board_stops_the_motor() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let lease = m.request_lease("mega.left", "a program", false).unwrap();
    m.command("mega.left", &lease.id, duty(0.8)).unwrap();
    let mut events = m.lease_events();
    let t = Instant::now();
    eventually("the motor to stop", || left(&wire) == 0.0);
    assert!(t.elapsed() < Duration::from_millis(250 + 100), "within the lease time: {:?}", t.elapsed());
    let ended = loop {
        if let Ok(LeaseEvent::Ended(e)) = events.try_recv() {
            break e;
        }
        assert!(t.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!((ended.lease.part.as_str(), ended.reason), ("mega.left", EndReason::Expired));
}

#[test]
fn fabers_estop_stops_the_board_and_refuses_leases_until_released() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let lease = m.request_lease("mega.right", "test", false).unwrap();
    m.command("mega.right", &lease.id, duty(-0.4)).unwrap();
    m.engage_estop(estop::Source::Shell, "Stop button");
    assert!(wire.board(|b| b.estop_engaged()));
    assert_eq!(wire.board(|b| b.output_of(Motor::Right)), 0.0);
    assert_eq!(m.request_lease("mega.right", "test", false).unwrap_err().code, "estop-engaged");
    m.release_estop(estop::Source::Shell).unwrap();
    assert!(!wire.board(|b| b.estop_engaged()));
    assert_eq!(wire.board(|b| b.output_of(Motor::Right)), 0.0, "releasing restarts nothing");
}

#[test]
fn the_boards_stop_button_engages_fabers_estop_which_stays_until_reset_on_the_board() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = machine_with(dir.path(), ScriptedBoard::one_motor());
    add_bridge(&m);
    status(&m, "mega.left", |s| *s == PartStatus::Running);
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    wire.board(|b| b.press_estop());
    eventually("Faber's e-stop", || m.estop().engaged.is_some());
    let engaged = m.estop().engaged.unwrap();
    assert_eq!(engaged.source, estop::Source::Part);
    assert!(engaged.reason.contains("mega"), "{engaged:?}");
    assert_eq!(m.command("mega.left", &lease.id, duty(0.1)).unwrap_err().code, "estop-engaged");
    eventually("the latch to show", || view(&m, "mega").unwrap().board.unwrap().hardware_estop);
    let e = m.release_estop(estop::Source::Shell).unwrap_err();
    assert_eq!((e.kind, e.code), (ErrorKind::Conflict, "estop-latched"));
    assert!(e.hint.contains("Reset it on the board"), "{e:?}");
    assert!(m.estop().engaged.is_some(), "still engaged");
    wire.board(|b| b.hardware_estop = false); // reset on the board
    eventually("the latch to clear", || !view(&m, "mega").unwrap().board.unwrap().hardware_estop);
    m.release_estop(estop::Source::Shell).unwrap();
    assert!(m.estop().engaged.is_none());
}

#[test]
fn a_boards_base_locks_the_motors_it_is_built_from() {
    let dir = tempfile::tempdir().unwrap();
    let base = Info::PlanarBase(planar_base::Info {
        holonomic: false,
        max_velocity: Twist2d { vx: 0.5, vy: 0.0, omega: 1.0 },
        max_acceleration: Twist2d { vx: 1.0, vy: 0.0, omega: 2.0 },
        built_from: vec!["left".into(), "right".into()],
    });
    let board =
        ScriptedBoard::new("sennep", vec![("left", motor_info()), ("right", motor_info()), ("base", base)]);
    let (m, wire) = machine_with(dir.path(), board);
    add_bridge(&m);
    status(&m, "mega.base", |s| *s == PartStatus::Running);
    let lease = m.request_lease("mega.base", "Drive window", false).unwrap();
    let locks: Vec<_> = lease.locks.iter().map(PartName::as_str).collect();
    assert_eq!(locks, ["mega.left", "mega.right"]);
    let e = m.request_lease("mega.left", "an app", false).unwrap_err();
    assert_eq!(e.code, "lease-held");
    assert!(e.message.contains("\"mega.left\" is part of \"mega.base\""), "{e:?}");
    let fast =
        Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d { vx: 2.0, vy: 0.0, omega: 0.0 }));
    let reply = m.command("mega.base", &lease.id, fast).unwrap();
    assert!(reply.clamped, "clamped to the board's own limits");
    assert_eq!(wire.board(|b| b.commands.last().map(|c| c.part)), Some(2));
    m.release_lease("mega.base", &lease.id).unwrap();
    let safes: Vec<_> = wire
        .sent()
        .into_iter()
        .filter_map(|m| if let Message::Safe(s) = m { s.part } else { None })
        .collect();
    assert_eq!(safes, [2, 0, 1], "the base, then the motors it is built from");
}

#[test]
fn unplugging_the_board_fails_the_bridge_stops_its_parts_and_ends_their_leases() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    let mut events = m.lease_events();
    wire.unplug();
    status(&m, "mega", |s| matches!(s, PartStatus::Failed(_)));
    status(&m, "mega.left", |s| matches!(s, PartStatus::Failed(_)));
    eventually("the lease to end", || matches!(events.try_recv(), Ok(LeaseEvent::Ended(_))));
    assert_eq!(
        m.command("mega.left", &lease.id, duty(0.2)).unwrap_err().lease_ended,
        Some(EndReason::PartFailed)
    );
    assert_eq!(view(&m, "mega").unwrap().board.unwrap().link, bridge::LinkState::Lost);
    wire.plug_in();
    status(&m, "mega.left", |s| *s == PartStatus::Running);
}

#[test]
fn removing_the_bridge_removes_its_sub_parts_and_ends_their_leases() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    m.command("mega.left", &lease.id, duty(0.5)).unwrap();
    m.remove_part("mega").unwrap();
    assert_eq!(left(&wire), 0.0, "the lease ended (SAFE) before the port closed");
    assert!(m.parts().is_empty());
    assert!(m.router().signals().is_empty());
    assert_eq!(m.request_lease("mega.left", "test", false).unwrap_err().code, "unknown-part");
}

#[test]
fn moving_a_sub_part_in_parts_toml_keeps_the_bridge_running() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    let path = dir.path().join("config/parts.toml");
    let text = std::fs::read_to_string(&path).unwrap() + "\n[parts.mega.parts.left]\nmount = { y = 0.25 }\n";
    m.write_file("home:/System/parts.toml", text.as_bytes(), WriteMode::Replace).unwrap();
    assert_eq!(view(&m, "mega.left").unwrap().mount.y, 0.25);
    m.command("mega.left", &lease.id, duty(0.3)).unwrap();
    assert_eq!(wire.opens(), 1, "not restarted");
}

#[test]
fn a_board_can_not_be_simulated() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _wire) = machine_with(dir.path(), FakeBoard::mega(250));
    let e = m
        .add_part(NewPart {
            address: None,
            name: "mega".into(),
            connector: BRIDGE_CONNECTOR.into(),
            port: None,
            simulate: Some(Simulate::Manual),
            mount: Mount::default(),
            settings: Settings::new(),
        })
        .unwrap_err();
    assert_eq!(e.code, "bridge-not-simulated");
    assert!(e.hint.contains("generic"), "{e:?}");
}

#[test]
fn checking_a_free_port_finds_a_board_and_suggests_it() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = machine_with(dir.path(), FakeBoard::mega(250));
    assert!(m.ports()[0].suggestions.is_empty(), "nothing is suggested before anyone asks");
    assert_eq!(wire.opens(), 0, "and the port is never opened on its own");
    let found = m.check_for_board(PORT).unwrap();
    assert_eq!(
        found,
        BoardCheck::Found {
            board: "sim-mega".into(),
            firmware: "faberlink-harness".into(),
            firmware_version: "0.1.0".into(),
            parts: 4
        }
    );
    let s = &m.ports()[0].suggestions[0];
    assert_eq!(s.connector, BRIDGE_CONNECTOR);
    assert_eq!(s.reason, "A FaberLink board: sim-mega (faberlink-harness 0.1.0).");
    assert_eq!(m.ports()[0].claimed_by, None, "the port is free again");
    assert!(wire.sent().iter().all(|m| matches!(m, Message::Hello(_))), "it only said hello");
}

#[test]
fn a_port_in_use_is_never_checked() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let opens = wire.opens();
    let e = m.check_for_board(PORT).unwrap_err();
    assert_eq!(e.code, "port-claimed");
    assert_eq!(wire.opens(), opens);
    assert_eq!(m.check_for_board("serial:NOPE").unwrap_err().code, "unknown-port");
}

fn parts_toml(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("config/parts.toml")).unwrap()
}

#[test]
fn pausing_a_bridge_frees_its_port_ends_its_leases_and_pauses_its_board_parts() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire) = mega(dir.path());
    let before = parts_toml(dir.path());
    let lease = m.request_lease("mega.left", "test", false).unwrap();
    m.command("mega.left", &lease.id, duty(0.5)).unwrap();
    let mut events = m.lease_events();

    let paused = m.pause_part("mega").unwrap();
    assert_eq!(paused.status, PartStatus::Paused);
    assert_eq!(left(&wire), 0.0, "the lease ended (SAFE) before the port closed");
    let ended = loop {
        match events.try_recv() {
            Ok(LeaseEvent::Ended(e)) => break e,
            Ok(_) => {}
            Err(e) => panic!("no lease end: {e:?}"),
        }
    };
    assert_eq!((ended.lease.part.as_str(), ended.reason), ("mega.left", EndReason::PartPaused));
    assert_eq!(m.ports()[0].claimed_by, None, "the port is free for the Arduino IDE");
    // The board's parts stay in the list, paused, so it is clear where they went.
    let names: Vec<_> = m.parts().into_iter().map(|p| (p.name.to_string(), p.status)).collect();
    assert_eq!(
        names,
        ["mega", "mega.left", "mega.right", "mega.front", "mega.rear"]
            .map(|n| (n.to_string(), PartStatus::Paused))
    );
    let e = m.request_lease("mega.left", "test", false).unwrap_err();
    assert_eq!((e.kind, e.code), (ErrorKind::Conflict, "part-paused"));
    assert!(e.hint.contains("Resume \"mega\""), "{e:?}");
    // The serial monitor can open the port now.
    drop(m.open_serial(PORT, 115_200).unwrap());

    // parts.toml says so, and nothing else in it changed.
    let text = parts_toml(dir.path());
    assert_eq!(
        text.replace(
            "paused = true
",
            ""
        ),
        before,
        "{text}"
    );
    assert_eq!(m.pause_part("mega").unwrap().status, PartStatus::Paused, "pausing twice is fine");

    let opens = wire.opens();
    let resumed = m.resume_part("mega").unwrap();
    assert_ne!(resumed.status, PartStatus::Paused);
    status(&m, "mega.left", |s| *s == PartStatus::Running);
    assert_eq!(wire.opens(), opens + 1);
    assert_eq!(m.ports()[0].claimed_by.as_deref(), Some("mega"));
    assert_eq!(parts_toml(dir.path()), before);
}

#[test]
fn a_paused_bridge_stays_paused_when_its_port_goes_and_comes_back_and_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (m, wire, port) = machine_on(dir.path(), FakeBoard::mega(250));
    add_bridge(&m);
    status(&m, "mega.left", |s| *s == PartStatus::Running);
    m.pause_part("mega").unwrap();
    let opens = wire.opens();
    port.gone.store(true, Ordering::Relaxed);
    m.refresh_ports();
    assert_eq!(view(&m, "mega").unwrap().status, PartStatus::Paused, "not \"port missing\"");
    port.gone.store(false, Ordering::Relaxed);
    m.refresh_ports();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(view(&m, "mega").unwrap().status, PartStatus::Paused);
    assert_eq!(wire.opens(), opens, "the port was never opened again");
    assert_eq!(m.ports()[0].claimed_by, None);
    m.shutdown();
    drop(m);

    let (m, wire) = machine_with(dir.path(), FakeBoard::mega(250));
    assert_eq!(view(&m, "mega").unwrap().status, PartStatus::Paused, "parts.toml remembers it");
    assert_eq!(wire.opens(), 0);
    m.resume_part("mega").unwrap();
    status(&m, "mega.left", |s| *s == PartStatus::Running);
}

#[test]
fn a_board_part_is_paused_with_its_board_and_unknown_parts_are_named() {
    let dir = tempfile::tempdir().unwrap();
    let (m, _wire) = mega(dir.path());
    let e = m.pause_part("mega.left").unwrap_err();
    assert_eq!((e.kind, e.code), (ErrorKind::Conflict, "pause-the-board"));
    assert!(e.hint.contains("\"mega\""), "{e:?}");
    assert_eq!(m.pause_part("nope").unwrap_err().code, "unknown-part");
    assert_eq!(m.resume_part("nope").unwrap_err().code, "unknown-part");
    assert_eq!(view(&m, "mega.left").unwrap().status, PartStatus::Running);
}
