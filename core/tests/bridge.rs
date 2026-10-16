//! Phase 4's proof (docs/roadmap.md): a Mega with FaberLink exposes motors and distance sensors,
//! a program holding a lease drives a motor, and killing the program stops the motor.
//!
//! The Mega is the Arduino library's real protocol core (`faberlink_interop::FakeBoard::mega`,
//! the same C++ that runs on the board) behind a fake serial port. The program is a client of
//! the HTTP API, as any app or script is. No hardware, no real COM port. The last test drives a
//! simulated SENNEP Mega through the SENNEP app.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_core::clock::{MonotonicClock, SharedClock};
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{
    Discovered, FailedUsb, PortEntry, PortOpener, PortReader, PortWriter, ReplayLibrary, SerialDiscovery,
    UsbProblemDiscovery,
};
use faber_core::runtime::Timing;
use faberlink::frame::Decoder;
use faberlink::message::kind;
use faberlink_interop::{FakeBoard, Motor, Sensor};
use serde_json::{Value, json};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// The lease time of a motor (docs/capabilities.md), and the board's own lease time for it.
const LEASE: Duration = Duration::from_millis(250);
/// What a stop may take beyond the lease: the sweeper (10 ms), the board's next poll, and a busy
/// test machine.
const SLACK: Duration = Duration::from_millis(120);

/// The Mega on its USB cable. The cable can be pulled, and can lose every SAFE frame.
struct Cable {
    board: Mutex<FakeBoard>,
    clock: SharedClock,
    plugged: AtomicBool,
    drop_safe: AtomicBool,
    /// SAFE frames the core sent, and those of them the cable lost.
    safes_sent: AtomicU32,
    safes_lost: AtomicU32,
}

impl Cable {
    fn now_ms(&self) -> u32 {
        (self.clock.now_us() / 1000) as u32
    }

    fn board<R>(&self, f: impl FnOnce(&mut FakeBoard) -> R) -> R {
        f(&mut self.board.lock().unwrap())
    }

    /// Counts the SAFE frames in `data`, and leaves them out while they are being lost.
    fn filter(&self, data: &[u8]) -> Vec<u8> {
        let lose = self.drop_safe.load(Ordering::Relaxed);
        let mut out = Vec::new();
        let (mut d, mut start) = (Decoder::new(), 0);
        for (i, &b) in data.iter().enumerate() {
            if let Some(f) = d.push(b) {
                let safe = matches!(f, Ok(f) if f.kind == kind::SAFE);
                if safe {
                    self.safes_sent.fetch_add(1, Ordering::Relaxed);
                }
                if safe && lose {
                    self.safes_lost.fetch_add(1, Ordering::Relaxed);
                } else {
                    out.extend_from_slice(&data[start..=i]);
                }
            }
            if b == 0 {
                start = i + 1;
            }
        }
        out.extend_from_slice(&data[start..]);
        out
    }
}

struct End(Arc<Cable>);

impl PortReader for End {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(1));
        if !self.0.plugged.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the device was unplugged"));
        }
        let now = self.0.now_ms();
        let out = self.0.board(|b| {
            b.tick(now);
            b.take_output()
        });
        buf[..out.len()].copy_from_slice(&out);
        Ok(out.len())
    }
}

impl PortWriter for End {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        if !self.0.plugged.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the device was unplugged"));
        }
        let now = self.0.now_ms();
        let data = self.0.filter(data);
        self.0.board(|b| b.feed(&data, now));
        Ok(())
    }
}

struct Opener(Arc<Cable>);

impl PortOpener for Opener {
    fn open(&self, _: &PortEntry, baud: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
        assert_eq!(baud, 115_200);
        if !self.0.plugged.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::NotFound, "the port is gone"));
        }
        Ok((Box::new(End(Arc::clone(&self.0))), Box::new(End(Arc::clone(&self.0)))))
    }
}

/// The Mega's USB port. Never a real one: the opener above is the only one this core has.
struct MegaPort;

impl SerialDiscovery for MegaPort {
    fn list(&self) -> Vec<Discovered> {
        vec![Discovered { path: "MEGA".into(), usb: None, bluetooth: false }]
    }
}
impl UsbProblemDiscovery for MegaPort {
    fn list(&self) -> Vec<FailedUsb> {
        Vec::new()
    }
}
impl DiskDiscovery for MegaPort {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    cable: Arc<Cable>,
    _dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

fn core() -> TestCore {
    core_with(FakeBoard::mega(250))
}

fn core_with(board: FakeBoard) -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let cable = Arc::new(Cable {
        board: Mutex::new(board),
        clock: Arc::clone(&clock),
        plugged: AtomicBool::new(true),
        drop_safe: AtomicBool::new(false),
        safes_sent: AtomicU32::new(0),
        safes_lost: AtomicU32::new(0),
    });
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: Arc::clone(&clock),
        discovery: Arc::new(MegaPort),
        usb_problems: Arc::new(MegaPort),
        disks: Arc::new(MegaPort),
        opener: Arc::new(Opener(Arc::clone(&cable))),
        replays: ReplayLibrary::default(),
        connectors: faber_core::connectors::built_in(),
        timing: Timing {
            backoff_start: Duration::from_millis(10),
            port_retry_max: Duration::from_millis(50),
            ..Timing::default()
        },
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    let app = crate::support::signed_in_api(&machine, LogBook::new(clock));
    TestCore { app, machine, cable, _dir: dir }
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    let body = match body {
        Some(b) => {
            req = req.header("content-type", "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, value)
}

async fn parts(c: &TestCore) -> Vec<Value> {
    call(&c.app, "GET", "/api/v0/parts", None).await.1.as_array().unwrap().clone()
}

async fn part(c: &TestCore, name: &str) -> Value {
    parts(c).await.into_iter().find(|p| p["name"] == name).unwrap_or(Value::Null)
}

async fn until(what: &str, mut f: impl AsyncFnMut() -> bool) {
    let start = Instant::now();
    while !f().await {
        assert!(start.elapsed() < Duration::from_secs(3), "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// Adds the Mega in the shell's way and waits for its parts.
async fn add_mega(c: &TestCore) {
    let (s, body) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "mega", "connector": "faber-bridge", "port": "serial:MEGA"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    until("the Mega's parts", async || part(c, "mega.rear").await["state"]["kind"] == "running").await;
}

fn duty(d: f32) -> Value {
    json!({"capability": "motor@1", "command": {"kind": "set-duty", "duty": d}})
}

fn left(c: &TestCore) -> f32 {
    c.cable.board(|b| b.output_of(Motor::Left))
}

/// A program driving one motor through the API: a lease, then a command every 50 ms (each one
/// renews the lease). Dropping it kills it: nothing more is sent, not even a release.
struct Program {
    task: tokio::task::JoinHandle<()>,
}

impl Program {
    async fn start(c: &TestCore, part: &str, d: f32) -> Program {
        let path = format!("/api/v0/parts/{part}/lease");
        let (s, grant) = call(&c.app, "POST", &path, Some(json!({"holder": "a program"}))).await;
        assert_eq!(s, StatusCode::CREATED, "{grant}");
        let lease = grant["lease"].as_str().unwrap().to_string();
        let (app, path) = (c.app.clone(), format!("/api/v0/parts/{part}/command"));
        let (s, reply) = call(&app, "POST", &path, Some(json!({"lease": lease, "command": duty(d)}))).await;
        assert_eq!(s, StatusCode::OK, "{reply}");
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let (s, _) =
                    call(&app, "POST", &path, Some(json!({"lease": lease, "command": duty(d)}))).await;
                if s != StatusCode::OK {
                    break; // e.g. the e-stop: a simple program just gives up
                }
            }
        });
        Program { task }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Kills the program and returns how long the motor kept turning after that.
async fn kill_and_time_the_stop(c: &TestCore, program: Program, motor: Motor) -> Duration {
    drop(program);
    let killed = Instant::now();
    until("the motor to stop", async || c.cable.board(|b| b.output_of(motor)) == 0.0).await;
    killed.elapsed()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mega_with_faberlink_shows_its_motors_and_distance_sensors() {
    let c = core();
    add_mega(&c).await;
    let all = parts(&c).await;
    let names: Vec<_> = all.iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["mega", "mega.left", "mega.right", "mega.front", "mega.rear"]);
    let mega = &all[0];
    assert_eq!(mega["board"]["link"], "ready");
    assert_eq!(mega["board"]["board"], "sim-mega");
    assert_eq!(mega["board"]["firmware"], "faberlink-harness");
    assert_eq!(mega["board"]["heartbeatTimeoutMs"], 250);
    assert_eq!(mega["board"]["hardwareEstop"], false);
    assert!(mega.get("bridge").is_none());
    let left = &all[1];
    assert_eq!(left["bridge"], "mega");
    assert_eq!(left["signals"][0]["address"], "mega.left/motor");
    assert_eq!(left["signals"][0]["info"]["info"]["maxVelocityRadS"], 10.0, "{left}");
    assert_eq!(all[3]["signals"][0]["address"], "mega.front/distance");

    c.cable.board(|b| b.set_distance_of(Sensor::Front, 0.37));
    until("a reading of 0.37 m", async || {
        let (s, v) = call(&c.app, "GET", "/api/v0/signals/mega.front/distance", None).await;
        s == StatusCode::OK
            && v["reading"]["reading"]["meters"].as_f64().is_some_and(|m| (m - 0.37).abs() < 1e-6)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_with_a_lease_drives_a_motor_and_killing_it_stops_the_motor_through_safe() {
    // The board's own lease is a whole second here, so a stop within 250 ms can only be SAFE.
    let c = core_with(FakeBoard::mega_with_motor_lease(250, 1000));
    add_mega(&c).await;
    let path = "/api/v0/parts/mega.left/command";
    let (s, e) = call(&c.app, "POST", path, Some(json!({"lease": "none", "command": duty(0.6)}))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("lease-required")));
    assert_eq!(left(&c), 0.0, "without a lease nothing reaches the board");

    let program = Program::start(&c, "mega.left", 0.6).await;
    assert_eq!(left(&c), 0.6, "the board's motor runs");
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(left(&c), 0.6, "and keeps running while the program renews by commanding");

    let took = kill_and_time_the_stop(&c, program, Motor::Left).await;
    assert!(took <= LEASE + SLACK, "stopped {took:?} after the program died");
    assert!(c.cable.safes_sent.load(Ordering::Relaxed) > 0, "stopped by SAFE");
    let (_, leases) = call(&c.app, "GET", "/api/v0/leases", None).await;
    assert_eq!(leases, json!([]), "the lease ended");
}

#[tokio::test(flavor = "multi_thread")]
async fn when_safe_is_lost_the_boards_own_lease_stops_the_motor() {
    let c = core();
    add_mega(&c).await;
    c.cable.drop_safe.store(true, Ordering::Relaxed);
    let program = Program::start(&c, "mega.right", -0.5).await;
    assert_eq!(c.cable.board(|b| b.output_of(Motor::Right)), -0.5);
    let took = kill_and_time_the_stop(&c, program, Motor::Right).await;
    assert!(took <= LEASE + SLACK, "stopped {took:?} after the program died");
    until("the core's SAFE to be sent (and lost)", async || c.cable.safes_lost.load(Ordering::Relaxed) > 0)
        .await;
    assert_eq!(left(&c), 0.0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_estop_stops_everything_on_the_board() {
    let c = core();
    add_mega(&c).await;
    let a = Program::start(&c, "mega.left", 0.7).await;
    let b = Program::start(&c, "mega.right", 0.7).await;
    let (s, status) = call(&c.app, "POST", "/api/v0/estop", Some(json!({"reason": "test"}))).await;
    assert_eq!((s, &status["engaged"]), (StatusCode::OK, &json!(true)));
    let outputs = c.cable.board(|b| (b.output_of(Motor::Left), b.output_of(Motor::Right), b.estop_engaged()));
    assert_eq!(outputs, (0.0, 0.0, true), "stopped at once, and the board holds the e-stop");
    drop((a, b));
    let (s, e) = call(&c.app, "POST", "/api/v0/parts/mega.left/lease", Some(json!({"holder": "x"}))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::LOCKED, Some("estop-engaged")));
    let (s, _) = call(&c.app, "POST", "/api/v0/estop/release", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!c.cable.board(|b| b.estop_engaged()));
    assert_eq!(left(&c), 0.0, "releasing restarts nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_boards_stop_button_engages_fabers_estop_until_it_is_reset() {
    let c = core();
    add_mega(&c).await;
    let program = Program::start(&c, "mega.left", 0.4).await;
    c.cable.board(|b| b.press_estop());
    until("Faber's e-stop", async || call(&c.app, "GET", "/api/v0/estop", None).await.1["engaged"] == true)
        .await;
    let (_, estop) = call(&c.app, "GET", "/api/v0/estop", None).await;
    assert_eq!(estop["engagedBy"]["source"], "part", "{estop}");
    assert_eq!(left(&c), 0.0);
    drop(program);
    until("the latch in Parts", async || part(&c, "mega").await["board"]["hardwareEstop"] == true).await;
    let (s, e) = call(&c.app, "POST", "/api/v0/estop/release", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("estop-latched")));
    c.cable.board(|b| b.reset_estop());
    until("the latch to clear", async || part(&c, "mega").await["board"]["hardwareEstop"] == false).await;
    let (s, _) = call(&c.app, "POST", "/api/v0/estop/release", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!c.cable.board(|b| b.estop_engaged()), "reset on the board, then released in Faber");
    let _again = Program::start(&c, "mega.left", 0.2).await;
    assert_eq!(left(&c), 0.2, "a new command drives again");
}

#[tokio::test(flavor = "multi_thread")]
async fn unplugging_the_board_fails_the_bridge_stops_its_parts_and_ends_every_lease() {
    let c = core();
    add_mega(&c).await;
    let a = Program::start(&c, "mega.left", 0.5).await;
    let (_, grant) =
        call(&c.app, "POST", "/api/v0/parts/mega.right/lease", Some(json!({"holder": "b"}))).await;
    c.cable.plugged.store(false, Ordering::Relaxed);
    a.task.abort(); // its next command would fail anyway
    until("the bridge to fail", async || part(&c, "mega").await["state"]["kind"] == "failed").await;
    for sub in ["mega.left", "mega.right", "mega.front", "mega.rear"] {
        let p = part(&c, sub).await;
        assert_eq!(p["state"]["kind"], "failed", "{p}");
        assert!(p["state"]["message"].as_str().unwrap().starts_with("Stopped"), "{p}");
    }
    until("every lease to end", async || call(&c.app, "GET", "/api/v0/leases", None).await.1 == json!([]))
        .await;
    let lease = grant["lease"].as_str().unwrap();
    let path = "/api/v0/parts/mega.right/command";
    let (s, e) = call(&c.app, "POST", path, Some(json!({"lease": lease, "command": duty(0.1)}))).await;
    assert_eq!((s, &e["error"]["reason"]), (StatusCode::FORBIDDEN, &json!("part-failed")), "{e}");
    let mega = part(&c, "mega").await;
    assert_eq!(mega["board"]["link"], "lost", "{mega}");
    assert!(mega["state"]["message"].as_str().unwrap().contains("MEGA"), "{mega}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_free_port_is_checked_for_a_board_only_when_asked() {
    let c = core();
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert_eq!(ports[0]["suggestions"], json!([]));
    let (s, found) =
        call(&c.app, "POST", "/api/v0/ports/check-board", Some(json!({"port": "serial:MEGA"}))).await;
    assert_eq!(s, StatusCode::OK, "{found}");
    assert_eq!(
        found,
        json!({"result": "found", "board": "sim-mega", "firmware": "faberlink-harness", "firmwareVersion": "0.1.0", "parts": 4})
    );
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert_eq!(ports[0]["suggestions"][0]["connector"], "faber-bridge");
    add_mega(&c).await;
    let (s, e) =
        call(&c.app, "POST", "/api/v0/ports/check-board", Some(json!({"port": "serial:MEGA"}))).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("port-claimed")),
        "a port in use is left alone"
    );
}

// ---- Phase 5: the SENNEP app drives a FaberLink board -----------------------------------------

mod support;

/// The SENNEP app (examples/apps/sennep) with its roles on SENNEP's Mega: the board's planar base,
/// its front distance sensor and its battery (and a simulated LiDAR, which the board doesn't
/// have). A client drives it the way the shell does for the app's window: through the app's
/// logic, with `drive` messages and the page's heartbeat over the WebSocket.
///
/// It proves that the app's commands reach the board, that the board's sensor and battery feed
/// its safety layer, and that the base on the board stops when the user presses the shell's Stop,
/// when the window goes quiet or when the app is turned off: through the core's SAFE, since the
/// board's own lease is a whole second here.
#[tokio::test(flavor = "multi_thread")]
async fn the_sennep_app_drives_the_boards_base_and_stops_it() {
    let Some(wasm) = support::component("sennep") else { return };
    let c = core_with(FakeBoard::sennep(250, 1000));
    let (s, body) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "mega", "connector": "faber-bridge", "port": "serial:MEGA"})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    until("the board's parts", async || part(&c, "mega.battery").await["state"]["kind"] == "running").await;
    let base = part(&c, "mega.base").await;
    assert_eq!(base["signals"][0]["capability"], "planar-base@1", "{base}");
    // Nothing in front of its distance sensor yet.
    c.cable.board(|b| b.set_distance_of(Sensor::Front, -1.0));

    // The board has no LiDAR: a simulated one in the empty world stands in for SENNEP's LD19.
    assert_eq!(call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "empty"}))).await.0, StatusCode::OK);
    let lidar =
        json!({"name": "lidar", "connector": "ldrobot-ld19", "simulate": "world", "mount": {"x": 0.2}});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(lidar)).await.0, StatusCode::CREATED);

    let req = Request::builder()
        .method("POST")
        .uri("/api/v0/packages?name=sennep-0.1.0.faberpkg")
        .header("content-type", "application/octet-stream")
        .body(Body::from(support::example_package("sennep", &wasm)))
        .unwrap();
    assert_eq!(c.app.clone().oneshot(req).await.unwrap().status(), StatusCode::CREATED);
    // The sensor's mount isn't set: it sits at SENNEP's centre, looking ahead.
    let roles = json!({"roles": {"lidar": "lidar", "base": "mega.base", "proximity": ["mega.front"], "battery": "mega.battery"}});
    let (s, a) = call(&c.app, "PUT", "/api/v0/apps/sennep/roles", Some(roles)).await;
    assert_eq!(s, StatusCode::OK, "{a}");
    call(&c.app, "POST", "/api/v0/apps/sennep/enable", None).await;
    until("the app to run", async || {
        call(&c.app, "GET", "/api/v0/apps/sennep", None).await.1["state"]["kind"] == "running"
    })
    .await;

    let vx = |c: &TestCore| c.cable.board(|b| b.base_velocity()[0]);
    let mut window = support::AppWindow::open(&c.app, "sennep").await;
    window.drive_via("base").await;
    window.hold(1.0, 0.0, 0.0);
    until("the board's base to drive at the top speed", async || (vx(&c) - 0.3).abs() < 1e-4).await;
    let leases = call(&c.app, "GET", "/api/v0/leases", None).await.1;
    assert_eq!(leases[0]["holder"], "app:sennep", "{leases}");

    // Something 0.1 m in front of SENNEP's front edge, seen by the board's own sensor: it stops.
    c.cable.board(|b| b.set_distance_of(Sensor::Front, 0.5));
    until("the board's sensor to stop it", async || vx(&c) == 0.0).await;
    c.cable.board(|b| b.set_distance_of(Sensor::Front, -1.0));
    until("it to drive on once clear", async || (vx(&c) - 0.3).abs() < 1e-4).await;

    // The board's battery goes critical (3S Li-ion below 9.9 V): 30 % of the top speed.
    c.cable.board(|b| b.set_battery_volts(9.6));
    until("the speed to be capped", async || (vx(&c) - 0.09).abs() < 1e-4).await;
    c.cable.board(|b| b.set_battery_volts(12.0));

    // The user presses the shell's Stop: the core ends the drive and the app's lease, SAFE.
    window.press_stop();
    let stop = Instant::now();
    until("the board's base to stop", async || vx(&c) == 0.0).await;
    assert!(stop.elapsed() < SLACK, "it took {:?}", stop.elapsed());

    // The page goes quiet (it hung): the core ends the drive, sends SAFE, the base stops.
    window.drive_via("base").await;
    until("it to drive again", async || vx(&c) > 0.0).await;
    window.hang();
    let quiet = Instant::now();
    until("the board's base to stop", async || vx(&c) == 0.0).await;
    assert!(quiet.elapsed() < Duration::from_millis(500), "it took {:?}", quiet.elapsed());
    drop(window);

    // Turned off while driving: at once.
    let mut window = support::AppWindow::open(&c.app, "sennep").await;
    window.drive_via("base").await;
    window.hold(1.0, 0.0, 0.0);
    until("it to drive again", async || vx(&c) > 0.0).await;
    let (s, _) = call(&c.app, "POST", "/api/v0/apps/sennep/disable", None).await;
    assert_eq!(s, StatusCode::OK);
    let off = Instant::now();
    until("the board's base to stop", async || vx(&c) == 0.0).await;
    assert!(off.elapsed() < SLACK, "it took {:?}", off.elapsed());
    drop(window);
}
