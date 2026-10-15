//! The simulated world through the API (docs/api.md, "The simulated world"), and Phase 2's
//! "done when" (docs/roadmap.md): SENNEP's parts are simulated in the apartment, a program drives
//! the base with the lease held, the simulated LiDAR scan shows the room, and dropping the lease
//! or pressing the e-stop stops the base. Everything runs on the core's own clock, at test speed.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::{Reading, distance, laser_scan, planar_base};
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::names::SignalAddress;
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::router::Subscription;
use faber_core::runtime::{PartStatus, Timing};
use faber_core::sim::world::Pose;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

struct NoPorts;
impl SerialDiscovery for NoPorts {
    fn list(&self) -> Vec<Discovered> {
        Vec::new()
    }
}
impl UsbProblemDiscovery for NoPorts {
    fn list(&self) -> Vec<FailedUsb> {
        Vec::new()
    }
}
impl DiskDiscovery for NoPorts {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    _dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(MonotonicClock::new());
    let replays = ReplayLibrary::with_demo();
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(NoPorts),
        usb_problems: Arc::new(NoPorts),
        disks: Arc::new(NoPorts),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, _dir: dir }
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

fn latest(c: &TestCore, address: &str) -> Option<Reading> {
    match c.machine.router().subscribe(&SignalAddress::parse(address).unwrap())? {
        Subscription::Latest(rx) => rx.borrow().as_deref().cloned(),
        Subscription::Every(_) => panic!("{address} is a latest-value signal"),
    }
}

fn now_us(c: &TestCore) -> u64 {
    c.machine.clock().now_us()
}

/// Waits (polling every 2 ms) until `f` gives something, or fails after `secs`.
async fn wait_for<T>(what: &str, secs: f32, mut f: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

// ---- the world API ----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn the_world_api_lists_shows_switches_and_resets() {
    let c = core();
    let (s, worlds) = call(&c.app, "GET", "/api/v0/worlds", None).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<_> = worlds.as_array().unwrap().iter().map(|w| w["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["empty", "apartment", "office", "warehouse", "maze", "obstacle-course"]);
    assert_eq!(worlds[1]["title"], "Apartment");
    assert_eq!(worlds[1]["size"], json!([10.0, 7.0]));

    let (s, world) = call(&c.app, "GET", "/api/v0/world", None).await;
    assert_eq!((s, world["name"].as_str()), (StatusCode::OK, Some("empty")), "the default");
    assert!(world.get("footprint").is_none(), "no world base yet");

    let (s, world) = call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "apartment"}))).await;
    assert_eq!(s, StatusCode::OK, "{world}");
    assert_eq!(world["title"], "Apartment");
    assert_eq!(world["start"], json!({"x": 7.5, "y": 2.5, "yaw": PI}));
    assert!(world["geometry"]["walls"].as_array().unwrap().len() > 4);
    assert_eq!(world["geometry"]["bounds"], json!({"min": [0.0, 0.0], "max": [10.0, 7.0]}));
    let parts_toml = std::fs::read_to_string(c._dir.path().join("config/parts.toml")).unwrap();
    assert!(parts_toml.contains("[world]\nname = \"apartment\""), "{parts_toml}");

    let start = json!({"x": 2.0, "y": 3.6, "yaw": 0.0});
    let (s, world) =
        call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "apartment", "start": start}))).await;
    assert_eq!((s, &world["start"]), (StatusCode::OK, &start));

    for (body, code) in [
        (json!({"name": "moon"}), "unknown-world"),
        (json!({"name": "apartment", "start": {"x": 20.0, "y": 1.0, "yaw": 0.0}}), "start-outside-world"),
        (json!({"start": {"x": 1.0}}), "bad-request"),
    ] {
        let (s, e) = call(&c.app, "PUT", "/api/v0/world", Some(body)).await;
        assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some(code)), "{e}");
        assert!(!e["error"]["hint"].as_str().unwrap().is_empty());
    }

    let base = json!({"name": "base", "connector": "generic-planar-base", "simulate": "world",
                      "settings": {"length": 0.8, "width": 0.65}});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(base)).await.0, StatusCode::CREATED);
    let (_, world) = call(&c.app, "GET", "/api/v0/world", None).await;
    assert_eq!(world["footprint"], json!({"length": 0.8, "width": 0.65}));
    assert_eq!(c.machine.world_state().pose, Pose::new(2.0, 3.6, 0.0));
    let (s, _) = call(&c.app, "POST", "/api/v0/world/reset", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn next(ws: &mut Ws) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Text(t) = ws.next().await.expect("socket open").unwrap() {
                return serde_json::from_str::<Value>(&t).unwrap();
            }
        }
    })
    .await
    .expect("a message within 5 s")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_world_topic_streams_the_machine_and_announces_switches() {
    let c = core();
    call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "obstacle-course"}))).await;
    let lidar = json!({"name": "lidar", "connector": "ldrobot-ld19", "simulate": "world"});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(lidar)).await.0, StatusCode::CREATED);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = c.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
    let sub = json!({"type": "subscribe", "id": 3, "topic": {"kind": "world"}});
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    let first = next(&mut ws).await;
    assert_eq!((first["type"].as_str(), first["id"].as_u64()), (Some("world"), Some(3)), "{first}");
    for key in ["pose", "velocity", "bumped", "movers", "tUs"] {
        assert!(first.get(key).is_some(), "{key} in {first}");
    }
    assert_eq!(first["movers"][0]["radius"], 0.25, "the obstacle course's person: {first}");
    let started = Instant::now();
    let mut n = 0;
    let mut walked = false;
    while started.elapsed() < Duration::from_millis(500) {
        let m = next(&mut ws).await;
        walked |=
            m["movers"][0]["x"] != first["movers"][0]["x"] || m["movers"][0]["y"] != first["movers"][0]["y"];
        n += 1;
    }
    assert!((5..=13).contains(&n), "20 a second: {n} in 0.5 s");
    assert!(walked, "the person walks");

    call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "maze"}))).await;
    let changed = wait_until_type(&mut ws, "world-changed").await;
    assert_eq!(changed, json!({"type": "world-changed", "id": 3, "name": "maze"}));
}

async fn wait_until_type(ws: &mut Ws, kind: &str) -> Value {
    loop {
        let m = next(ws).await;
        if m["type"] == kind {
            return m;
        }
    }
}

// ---- Phase 2, done when -----------------------------------------------------------------------

/// SENNEP V2 simulated in the apartment: its 0.8 × 0.65 m base, the LD19 0.2 m ahead of the
/// centre, and a distance sensor at each corner looking out diagonally.
async fn sennep_in_the_apartment() -> TestCore {
    let c = core();
    let (s, _) = call(&c.app, "PUT", "/api/v0/world", Some(json!({"name": "apartment"}))).await;
    assert_eq!(s, StatusCode::OK);
    let mut parts = vec![
        json!({"name": "base", "connector": "generic-planar-base", "simulate": "world",
               "settings": {"length": 0.8, "width": 0.65, "max-vx": 0.6, "max-vy": 0.6, "max-omega": 1.5, "max-accel": 1.0}}),
        json!({"name": "lidar", "connector": "ldrobot-ld19", "simulate": "world", "mount": {"x": 0.2, "z": 0.3}}),
    ];
    for (name, x, y, yaw) in [
        ("front_left", 0.4, 0.325, FRAC_PI_4),
        ("front_right", 0.4, -0.325, -FRAC_PI_4),
        ("rear_left", -0.4, 0.325, 3.0 * FRAC_PI_4),
        ("rear_right", -0.4, -0.325, -3.0 * FRAC_PI_4),
    ] {
        parts.push(json!({"name": name, "connector": "generic-distance", "simulate": "world",
                          "mount": {"x": x, "y": y, "z": 0.2, "yaw": yaw}}));
    }
    for p in parts {
        let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(p)).await;
        assert_eq!(s, StatusCode::CREATED, "{body}");
    }
    for p in c.machine.parts() {
        assert_eq!(p.status, PartStatus::Running, "{}", p.name);
    }
    c
}

fn base(c: &TestCore) -> planar_base::Reading {
    match latest(c, "base/planar-base") {
        Some(Reading::PlanarBase(r)) => r,
        other => panic!("no base reading: {other:?}"),
    }
}

fn pose(c: &TestCore) -> (f64, f64, f64) {
    match latest(c, "world/pose") {
        Some(Reading::Pose(p)) => {
            let q = p.orientation;
            let yaw = (2.0 * (q.w * q.z + q.x * q.y)).atan2(1.0 - 2.0 * (q.y * q.y + q.z * q.z));
            (f64::from(p.position.x), f64::from(p.position.y), f64::from(yaw))
        }
        other => panic!("no world/pose: {other:?}"),
    }
}

/// The first scan that started after `after_us`.
async fn scan_after(c: &TestCore, after_us: u64) -> laser_scan::Reading {
    wait_for("a fresh scan", 2.0, || match latest(c, "lidar/laser-scan") {
        Some(Reading::LaserScan(s)) if s.t_us > after_us => Some(s),
        _ => None,
    })
    .await
}

/// The scan's range in direction `angle` (rad, the LiDAR's frame: 0 ahead, counter-clockwise).
fn range_towards(s: &laser_scan::Reading, angle: f64) -> f64 {
    let off = |i: usize| {
        (f64::from(s.angle(i)) - angle)
            .rem_euclid(std::f64::consts::TAU)
            .min((angle - f64::from(s.angle(i))).rem_euclid(std::f64::consts::TAU))
    };
    let i = (0..s.len()).min_by(|&a, &b| off(a).total_cmp(&off(b))).unwrap();
    f64::from(s.ranges[i])
}

/// What the apartment shows a LiDAR at `(x, 2.5)` facing west, the way SENNEP drives from the
/// start: the living room's west wall (x = 6) ahead, the south wall (y = 0) on the left, and the
/// sofa (x = 9.1) behind; each as `(what, angle, nearest, farthest)`. To the right (north) the
/// dining chairs' and table's thin legs may be hit, from 4.74 m north, before the north wall.
fn apartment_ranges(lidar_x: f64) -> [(&'static str, f64, f64, f64); 4] {
    let tol = 0.05;
    let exactly = |r: f64| (r - tol, r + tol);
    let (ahead, left, behind) = (exactly(lidar_x - 6.0), exactly(2.5), exactly(9.1 - lidar_x));
    [
        ("west wall ahead", 0.0, ahead.0, ahead.1),
        ("south wall left", FRAC_PI_2, left.0, left.1),
        ("sofa behind", PI, behind.0, behind.1),
        ("legs or the north wall right", -FRAC_PI_2, 4.74 - 2.5 - tol, 7.0 - 2.5 + tol),
    ]
}

fn assert_scan_shows_the_room(s: &laser_scan::Reading, machine_x: f64) {
    assert_eq!(s.len(), 450, "the LD19's 0.8° resolution");
    for (what, angle, nearest, farthest) in apartment_ranges(machine_x - 0.2) {
        let got = range_towards(s, angle);
        assert!(
            (nearest..=farthest).contains(&got),
            "{what}: {got:.3} m, the apartment says {nearest:.3}–{farthest:.3} m"
        );
    }
}

fn drive(vx: f64) -> Value {
    json!({"capability": "planar-base@1", "command": {"kind": "set-velocity", "vx": vx, "vy": 0.0, "omega": 0.0}})
}

async fn take_lease(c: &TestCore) -> String {
    let (s, grant) =
        call(&c.app, "POST", "/api/v0/parts/base/lease", Some(json!({"holder": "done-when"}))).await;
    assert_eq!(s, StatusCode::CREATED, "{grant}");
    grant["lease"].as_str().unwrap().to_string()
}

/// Sends `vx` every 50 ms for `secs`, as a program does. Returns the last lease deadline.
async fn hold(c: &TestCore, lease: &str, vx: f64, secs: f32) -> u64 {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    let mut deadline = 0;
    while Instant::now() < until {
        let body = json!({"lease": lease, "command": drive(vx)});
        let (s, reply) = call(&c.app, "POST", "/api/v0/parts/base/command", Some(body)).await;
        assert_eq!(s, StatusCode::OK, "{reply}");
        deadline = reply["deadlineUs"].as_u64().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    deadline
}

/// Waits until the base reports `measured` 0; returns that reading's time.
async fn stopped_at(c: &TestCore) -> u64 {
    wait_for("the base to stop", 3.0, || {
        let r = base(c);
        (r.measured.unwrap().vx == 0.0).then_some(r.t_us)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn sennep_drives_in_the_apartment_and_its_lidar_sees_the_room() {
    let c = sennep_in_the_apartment().await;
    let (x0, y0, yaw0) = pose(&c);
    assert_eq!((x0, y0), (7.5, 2.5), "at the apartment's start");
    assert!((yaw0.abs() - PI).abs() < 1e-5, "facing west: {yaw0}");
    assert_scan_shows_the_room(&scan_after(&c, now_us(&c)).await, x0);
    for corner in ["front_left", "front_right", "rear_left", "rear_right"] {
        let Some(Reading::Distance(d)) = latest(&c, &format!("{corner}/distance")) else {
            panic!("{corner}")
        };
        assert!(d.meters.is_finite() && d.status != distance::Status::Error, "{corner}: {d:?}");
    }

    let lease = take_lease(&c).await;
    hold(&c, &lease, 0.4, 1.0).await;
    assert!(base(&c).measured.unwrap().vx > 0.3, "it got up to speed");
    let (s, _) = call(&c.app, "DELETE", &format!("/api/v0/parts/base/lease?lease={lease}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "done driving: let go");
    let t = stopped_at(&c).await;
    let (x1, y1, _) = pose(&c);
    assert!(x0 - x1 > 0.25 && x0 - x1 < 0.6, "drove about 0.4 m west: {x0} → {x1}");
    assert_eq!(y1, y0, "straight");
    assert_scan_shows_the_room(&scan_after(&c, t).await, x1);
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_lease_stops_the_base_within_the_lease_time_and_its_ramp() {
    let c = sennep_in_the_apartment().await;
    let lease = take_lease(&c).await;
    let deadline = hold(&c, &lease, 0.5, 0.8).await;
    assert_eq!(base(&c).measured.unwrap().vx, 0.5, "at full speed");
    // The program goes quiet: no command, no renewal, no release.
    let t = stopped_at(&c).await;
    // 250 ms lease, then the sweeper (≤ 50 ms), then 0.5 m/s at 1 m/s² takes 0.5 s; plus a
    // publishing period (50 ms) and scheduling.
    assert!(t > deadline + 400_000, "it ramps down instead of stopping dead: {} ms", (t - deadline) / 1000);
    assert!(t < deadline + 50_000 + 500_000 + 150_000, "{} ms after the deadline", (t - deadline) / 1000);
    assert_eq!(base(&c).commanded.vx, 0.0, "the safe state");
    let (x, _, _) = pose(&c);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(pose(&c).0, x, "and it stays put");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_estop_stops_the_base_within_its_ramp() {
    let c = sennep_in_the_apartment().await;
    let lease = take_lease(&c).await;
    hold(&c, &lease, 0.5, 0.8).await;
    let pressed = now_us(&c);
    let (s, _) = call(&c.app, "POST", "/api/v0/estop", Some(json!({"reason": "test"}))).await;
    assert_eq!(s, StatusCode::OK);
    let r = base(&c);
    assert_eq!(r.commanded.vx, 0.0, "the zero command arrives at once");
    assert!(r.measured.unwrap().vx > 0.3, "but it can't stop dead");
    let t = stopped_at(&c).await;
    assert!(t < pressed + 500_000 + 150_000, "stopped {} ms after the e-stop", (t - pressed) / 1000);
    let body = json!({"lease": lease, "command": drive(0.5)});
    let (s, _) = call(&c.app, "POST", "/api/v0/parts/base/command", Some(body)).await;
    assert_eq!(s, StatusCode::LOCKED);
}

#[tokio::test(flavor = "multi_thread")]
async fn driving_into_a_wall_stops_with_bumped_and_never_passes_through() {
    let c = sennep_in_the_apartment().await;
    let lease = take_lease(&c).await;
    // West at 0.6 m/s: the front edge (x = 7.1) meets the living room's west wall (x = 6) in
    // about 2 s. Keep pushing, renewing the lease, and watch every pose on the way.
    let started = Instant::now();
    let mut front = f64::INFINITY;
    loop {
        let body = json!({"lease": lease, "command": drive(0.6)});
        assert_eq!(call(&c.app, "POST", "/api/v0/parts/base/command", Some(body)).await.0, StatusCode::OK);
        for _ in 0..10 {
            front = front.min(pose(&c).0 - 0.4);
            assert!(front >= 6.0 - 1e-6, "the front edge passed the wall: x = {front}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if base(&c).bumped {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(6), "never reached the wall");
    }
    let r = base(&c);
    assert_eq!((r.commanded.vx, r.measured.unwrap().vx), (0.6, 0.0), "pushing, but stopped");
    assert!(front < 6.0 + 0.02, "it got right up to the wall: {front}");
    hold(&c, &lease, 0.6, 0.3).await;
    assert!(base(&c).bumped, "still bumped while it pushes");
    assert!((pose(&c).0 - 0.4 - front).abs() < 1e-3, "and it doesn't move");
    let ahead = range_towards(&scan_after(&c, now_us(&c)).await, 0.0);
    assert!((ahead - 0.2).abs() < 0.05, "the LiDAR, 0.2 m behind the front edge, agrees: {ahead}");
}
