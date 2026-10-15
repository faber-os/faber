use super::testboard::{ScriptedBoard, Wire, motor_info};
use super::*;
use crate::clock::MonotonicClock;
use faber_capabilities::{Info, Twist2d, motor, planar_base};
use faberlink::Message;
use faberlink_interop::{FakeBoard, Motor, Sensor};
use std::collections::BTreeMap;

#[derive(Default)]
struct Outlet {
    health: Mutex<BTreeMap<PartName, Health>>,
    forgotten: Mutex<Vec<PartName>>,
    events: Mutex<Vec<BridgeEvent>>,
}

impl BridgeOutlet for Outlet {
    fn set_health(&self, part: &PartName, health: Health) {
        self.health.lock().unwrap().insert(part.clone(), health);
    }
    fn forget(&self, part: &PartName) {
        self.health.lock().unwrap().remove(part);
        self.forgotten.lock().unwrap().push(part.clone());
    }
    fn event(&self, event: BridgeEvent) {
        self.events.lock().unwrap().push(event);
    }
}

impl Outlet {
    fn health(&self, part: &str) -> Option<Health> {
        self.health.lock().unwrap().get(&PartName::parse_any(part).unwrap()).cloned()
    }
}

fn name(s: &str) -> PartName {
    PartName::parse_any(s).unwrap()
}

fn timing() -> BridgeTiming {
    BridgeTiming {
        backoff_start: Duration::from_millis(10),
        retry_max: Duration::from_millis(60),
        handshake: Duration::from_millis(200),
        status_silence: Duration::from_millis(400),
        session: SessionConfig::default(),
    }
}

struct Rig<B: Board> {
    wire: Wire<B>,
    outlet: Arc<Outlet>,
    router: Arc<Router>,
    handle: BridgeHandle,
}

use super::testboard::Board;

fn rig<B: Board>(board: B) -> Rig<B> {
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let wire = Wire::new(board, Arc::clone(&clock));
    let outlet = Arc::new(Outlet::default());
    let router = Arc::new(Router::new());
    let deps = BridgeDeps {
        clock,
        opener: Arc::new(wire.clone()),
        router: Arc::clone(&router),
        outlet: Arc::clone(&outlet) as Arc<dyn BridgeOutlet>,
        timing: timing(),
    };
    let handle = start(BridgeSpec { name: name("mega"), port: Wire::<B>::entry() }, deps, None);
    Rig { wire, outlet, router, handle }
}

/// Polls `f` until it holds, or fails after 3 s with what it last saw.
fn eventually<T: std::fmt::Debug>(what: &str, mut f: impl FnMut() -> (bool, T)) -> T {
    let start = Instant::now();
    loop {
        let (ok, seen) = f();
        if ok {
            return seen;
        }
        assert!(start.elapsed() < Duration::from_secs(3), "timed out waiting for {what}; last saw {seen:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn running<B: Board>(r: &Rig<B>, part: &str) {
    eventually(&format!("{part} running"), || {
        let h = r.outlet.health(part);
        (h == Some(Health::Running), h)
    });
}

fn failed<B: Board>(r: &Rig<B>, part: &str) -> Problem {
    eventually(&format!("{part} failed"), || match r.outlet.health(part) {
        Some(Health::Failed(p)) => (true, Some(p)),
        h => (false, h.map(|_| Problem::new("not failed", ""))),
    })
    .unwrap()
}

fn duty(d: f32) -> Command {
    Command::Motor(motor::Command::SetDuty { duty: d })
}

#[test]
fn the_boards_parts_become_sub_parts_whose_readings_reach_their_signals() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega");
    running(&r, "mega.left");
    let names: Vec<_> = r.handle.sub_parts().into_iter().map(|s| s.name.to_string()).collect();
    assert_eq!(names, ["mega.left", "mega.right", "mega.front", "mega.rear"]);
    let view = r.handle.view();
    assert_eq!(view.link, LinkState::Ready);
    assert_eq!(view.board.unwrap().board, "sim-mega");
    r.wire.board(|b| b.set_distance_of(Sensor::Rear, 0.42));
    let rear = SignalAddress::parse("mega.rear/distance").unwrap();
    eventually("a rear reading of 0.42 m", || {
        let latest = r.router.latest(&rear).flatten();
        let m = latest.as_deref().map(|r| match r {
            Reading::Distance(d) => d.meters,
            _ => -1.0,
        });
        (m == Some(0.42), m)
    });
    let declared: Vec<_> = r.router.signals().into_iter().map(|(a, _)| a.to_string()).collect();
    assert!(declared.contains(&"mega/estop".to_string()), "{declared:?}");
}

#[test]
fn commands_reach_the_board_and_safe_stops_the_part() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega.left");
    assert_eq!(r.handle.command(&name("mega.right"), duty(0.5)), Ok(Applied { command: duty(0.5) }));
    assert_eq!(r.wire.board(|b| (b.output_of(Motor::Left), b.output_of(Motor::Right))), (0.0, 0.5));
    r.handle.safe(&name("mega.right"));
    assert_eq!(r.wire.board(|b| b.output_of(Motor::Right)), 0.0);
    let Err(CommandError::NotReady(p)) = r.handle.command(&name("mega.nope"), duty(0.1)) else { panic!() };
    assert!(p.message.contains("no part \"nope\""), "{p:?}");
    let stop_base = Command::PlanarBase(planar_base::Command::STOP);
    assert_eq!(r.handle.command(&name("mega.left"), stop_base), Err(CommandError::Unsupported));
}

#[test]
fn fabers_estop_reaches_the_board_and_its_release_too() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega.left");
    r.handle.command(&name("mega.left"), duty(0.5)).unwrap();
    r.handle.set_estop(Some((estop::Source::Shell, "Stop button".into())));
    assert!(r.wire.board(|b| b.estop_engaged()));
    assert_eq!(r.wire.board(|b| b.output_of(Motor::Left)), 0.0);
    r.handle.set_estop(None);
    assert!(!r.wire.board(|b| b.estop_engaged()));
}

#[test]
fn a_board_that_connects_while_the_estop_is_engaged_hears_it_in_its_first_heartbeat() {
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let wire = Wire::new(FakeBoard::mega(250), Arc::clone(&clock));
    let outlet = Arc::new(Outlet::default());
    let deps = BridgeDeps {
        clock,
        opener: Arc::new(wire.clone()),
        router: Arc::new(Router::new()),
        outlet: Arc::clone(&outlet) as Arc<dyn BridgeOutlet>,
        timing: timing(),
    };
    let _h = start(
        BridgeSpec { name: name("mega"), port: Wire::<FakeBoard>::entry() },
        deps,
        Some((estop::Source::App, "test".into())),
    );
    eventually("the board to hold the e-stop", || {
        let e = wire.board(|b| b.estop_engaged());
        (e, e)
    });
}

#[test]
fn a_port_without_a_faberlink_board_says_what_to_check() {
    let mut b = ScriptedBoard::one_motor();
    b.mute = true;
    let r = rig(b);
    let p = failed(&r, "mega");
    assert_eq!(p.message, "No FaberLink answer from FAKE.");
    assert!(p.hint.contains("FaberLink sketch") && p.hint.contains("baud"), "{p:?}");
    assert_eq!(r.handle.view().link, LinkState::NoAnswer);
    r.wire.board(|b| b.mute = false);
    running(&r, "mega");
}

#[test]
fn a_board_with_another_protocol_version_says_what_to_update_and_is_asked_again() {
    let mut b = ScriptedBoard::one_motor();
    b.versions = (2, 3);
    let r = rig(b);
    let p = failed(&r, "mega");
    assert!(
        p.message.contains("speaks FaberLink versions 2–3") && p.message.contains("Update Faber"),
        "{p:?}"
    );
    assert_eq!(r.handle.view().link, LinkState::Incompatible);
    r.wire.board(|b| b.versions = (1, 1)); // the board was flashed with this Faber's library
    running(&r, "mega.left");
}

#[test]
fn unplugging_fails_the_bridge_stops_its_parts_and_plugging_in_brings_them_back() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega.left");
    r.wire.unplug();
    let p = failed(&r, "mega");
    assert!(p.message.contains("FAKE stopped working") || p.message.contains("Could not open FAKE"), "{p:?}");
    let sub = failed(&r, "mega.left");
    assert!(sub.message.starts_with("Stopped:"), "{sub:?}");
    assert_eq!(r.handle.sub_parts().len(), 4, "the parts stay, stopped, while the board is away");
    assert!(matches!(r.handle.command(&name("mega.left"), duty(0.2)), Err(CommandError::NotReady(_))));
    r.wire.plug_in();
    running(&r, "mega.left");
    running(&r, "mega");
    assert!(r.wire.opens() >= 2);
}

#[test]
fn new_firmware_with_other_parts_is_described_again_on_reconnect() {
    let r = rig(ScriptedBoard::one_motor());
    running(&r, "mega.left");
    r.outlet.events.lock().unwrap().clear();
    r.wire.board(|b| b.parts = vec![("right".into(), motor_info())]);
    r.wire.unplug();
    failed(&r, "mega");
    r.wire.plug_in();
    running(&r, "mega.right");
    let names: Vec<_> = r.handle.sub_parts().into_iter().map(|s| s.name.to_string()).collect();
    assert_eq!(names, ["mega.right"]);
    assert!(r.outlet.forgotten.lock().unwrap().contains(&name("mega.left")));
    assert_eq!(r.outlet.health("mega.left"), None);
    let signals: Vec<_> = r.router.signals().into_iter().map(|(a, _)| a.to_string()).collect();
    assert!(signals.contains(&"mega.right/motor".into()) && !signals.contains(&"mega.left/motor".into()));
    let change =
        BridgeEvent::PartsChanged { bridge: name("mega"), removed: vec![name("mega.left")], changed: vec![] };
    assert!(r.outlet.events.lock().unwrap().contains(&change), "{:?}", r.outlet.events.lock().unwrap());
}

#[test]
fn the_same_parts_on_reconnect_are_no_change() {
    let r = rig(ScriptedBoard::one_motor());
    running(&r, "mega.left");
    r.outlet.events.lock().unwrap().clear();
    r.wire.unplug();
    failed(&r, "mega.left");
    r.wire.plug_in();
    running(&r, "mega.left");
    assert!(r.outlet.events.lock().unwrap().is_empty(), "{:?}", r.outlet.events.lock().unwrap());
}

#[test]
fn the_boards_stop_button_engages_fabers_estop_and_stays_latched_until_reset_on_the_board() {
    let r = rig(ScriptedBoard::one_motor());
    running(&r, "mega.left");
    r.wire.board(|b| b.press_estop());
    let events = eventually("a hardware e-stop", || {
        let e = r.outlet.events.lock().unwrap().clone();
        (e.iter().any(|e| matches!(e, BridgeEvent::HardwareEstop { .. })), e)
    });
    let presses =
        |e: &[BridgeEvent]| e.iter().filter(|e| matches!(e, BridgeEvent::HardwareEstop { .. })).count();
    let Some(BridgeEvent::HardwareEstop { bridge, reason }) =
        events.iter().find(|e| matches!(e, BridgeEvent::HardwareEstop { .. }))
    else {
        unreachable!()
    };
    assert_eq!(bridge.as_str(), "mega");
    assert!(reason.contains("mega"), "{reason}");
    assert!(r.handle.hardware_estop_latched() && r.handle.view().hardware_estop);
    let own = SignalAddress::parse("mega/estop").unwrap();
    let Some(Some(reading)) = r.router.latest(&own) else { panic!("no e-stop reading") };
    let Reading::Estop(e) = &*reading else { panic!() };
    assert!(e.engaged && e.source == estop::Source::Part);
    let Health::Degraded(p) = eventually("the bridge degraded", || {
        let h = r.outlet.health("mega");
        (matches!(h, Some(Health::Degraded(_))), h.unwrap())
    }) else {
        unreachable!()
    };
    assert!(p.hint.contains("Reset it on the board"), "{p:?}");
    assert_eq!(presses(&r.outlet.events.lock().unwrap()), 1, "STATUS saying so again is not a new press");
    r.wire.board(|b| b.hardware_estop = false); // reset on the board
    eventually("the latch to clear", || {
        let l = r.handle.hardware_estop_latched();
        (!l, l)
    });
    running(&r, "mega");
}

#[test]
fn a_board_that_reports_a_fault_fails_with_what_went_wrong() {
    let r = rig(ScriptedBoard::one_motor());
    running(&r, "mega");
    r.wire.board(|b| b.fault = true);
    let p = failed(&r, "mega");
    assert_eq!(p.message, "The board reports a fault: it refused a command.");
    r.wire.board(|b| b.fault = false);
    running(&r, "mega");
}

#[test]
fn a_board_that_stops_sending_its_status_is_degraded() {
    let r = rig(ScriptedBoard::one_motor());
    running(&r, "mega");
    r.wire.board(|b| b.no_status = true);
    let h = eventually("degraded", || {
        let h = r.outlet.health("mega");
        (matches!(h, Some(Health::Degraded(_))), h)
    });
    let Some(Health::Degraded(p)) = h else { unreachable!() };
    assert!(p.message.contains("stopped sending its status"), "{p:?}");
}

#[test]
fn a_bases_lease_reaches_the_motors_its_board_builds_it_from() {
    let base = Info::PlanarBase(planar_base::Info {
        holonomic: false,
        max_velocity: Twist2d { vx: 0.5, vy: 0.0, omega: 1.0 },
        max_acceleration: Twist2d { vx: 1.0, vy: 0.0, omega: 2.0 },
        built_from: vec!["left".into(), "right".into()],
    });
    let board =
        ScriptedBoard::new("sennep", vec![("left", motor_info()), ("right", motor_info()), ("base", base)]);
    let r = rig(board);
    running(&r, "mega.base");
    let base = r.handle.sub_part(&name("mega.base")).unwrap();
    let info = base.provide.unwrap().info.unwrap();
    assert_eq!(info.built_from(), ["mega.left", "mega.right"]);
    let twist =
        Command::PlanarBase(planar_base::Command::SetVelocity(Twist2d { vx: 0.2, vy: 0.0, omega: 0.0 }));
    r.handle.command(&name("mega.base"), twist).unwrap();
    assert_eq!(r.wire.board(|b| b.commands.last().map(|c| c.part)), Some(2));
}

#[test]
fn stopping_a_bridge_forgets_its_parts_and_removes_their_signals() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega.left");
    let Rig { handle, outlet, router, .. } = r;
    handle.stop();
    assert!(router.signals().is_empty(), "{:?}", router.signals());
    assert_eq!(outlet.forgotten.lock().unwrap().len(), 4);
}

#[test]
fn only_leased_commands_and_safe_states_are_sent_never_anything_unasked() {
    let r = rig(FakeBoard::mega(250));
    running(&r, "mega.left");
    std::thread::sleep(Duration::from_millis(200));
    let sent = r.wire.sent();
    assert!(
        sent.iter().all(|m| matches!(m, Message::Hello(_) | Message::Describe | Message::Heartbeat(_))),
        "{sent:?}"
    );
    let heartbeats = sent.iter().filter(|m| matches!(m, Message::Heartbeat(_))).count();
    assert!(heartbeats >= 3, "a heartbeat every 50 ms: {heartbeats}");
}

mod probing {
    use super::*;
    use crate::bridge::probe::{ProbeOutcome, ProbeTiming, probe};

    fn run<B: Board>(board: B) -> ProbeOutcome {
        let clock: SharedClock = Arc::new(MonotonicClock::new());
        let wire = Wire::new(board, Arc::clone(&clock));
        let (reader, mut writer) = wire.open(&Wire::<B>::entry(), BAUD).unwrap();
        let timing =
            ProbeTiming { window: Duration::from_millis(300), hello_every: Duration::from_millis(50) };
        let out = probe(reader, &mut *writer, &clock, &timing).unwrap();
        let sent = wire.sent();
        assert!(sent.iter().all(|m| matches!(m, Message::Hello(_))), "a probe only says hello: {sent:?}");
        out
    }

    #[test]
    fn a_faberlink_board_answers_with_its_name_and_firmware() {
        let ProbeOutcome::Found(info) = run(FakeBoard::mega(250)) else { panic!() };
        assert_eq!(
            (info.board.as_str(), info.firmware.as_str(), info.part_count),
            ("sim-mega", "faberlink-harness", 4)
        );
    }

    #[test]
    fn anything_else_is_nothing() {
        let mut b = ScriptedBoard::one_motor();
        b.mute = true;
        let t = Instant::now();
        assert_eq!(run(b), ProbeOutcome::Nothing);
        assert!(t.elapsed() < Duration::from_secs(1), "gives up after its window");
    }

    #[test]
    fn a_board_with_another_version_is_still_a_faberlink_board() {
        let mut b = ScriptedBoard::one_motor();
        b.versions = (2, 2);
        let ProbeOutcome::OtherVersion { message } = run(b) else { panic!() };
        assert!(message.contains("Update Faber"), "{message}");
    }
}
