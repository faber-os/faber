//! Apps (docs/apps.md), through the API. The proof of Phase 5a: the wall-follower example, built
//! as a component and packed, is installed on a running core, its roles are bound to a simulated
//! SENNEP in the apartment, and it drives the base along a wall without bumping while publishing
//! its derived signal. Turning it off, a part leaving a role, an operator taking over and a trap
//! all end its lease and stop the base.
//!
//! The components are built here from the workspace's own crates (`support`). Without the
//! `wasm32-unknown-unknown` target the tests say so and pass; CI sets `FABER_REQUIRE_WASM=1`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::{Reading, planar_base};
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::names::SignalAddress;
use faber_core::packages;
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::router::Subscription;
use faber_core::runtime::Timing;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

mod support;
use support::{check_e2e_fixture, component, example_package};

fn wall_follower_package(wasm: &[u8]) -> Vec<u8> {
    example_package("wall-follower", wasm)
}

/// The package the shell's end-to-end test installs (shell/e2e/apps.spec.ts): the wall-follower
/// example, packed.
#[test]
fn the_e2e_package_is_the_packed_wall_follower() {
    let Some(wasm) = component("wall-follower") else { return };
    check_e2e_fixture("wall-follower-0.1.0.faberpkg", wall_follower_package(&wasm));
}

const TRAPPER: &str = "faber-manifest = 1\n[package]\nid = \"trapper\"\ntype = \"app\"\nversion = \"0.1.0\"\n\
    name = \"Trapper\"\nmodule = \"app.wasm\"\ntick-hz = 50\n\
    [[requires]]\nrole = \"base\"\ncapability = \"planar-base@1\"\ncommand = true\n";

fn trapper_package(wasm: &[u8]) -> Vec<u8> {
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", TRAPPER.as_bytes()),
        ("README.md", b"# Trapper".as_slice()),
        ("LICENSE", b"MIT".as_slice()),
        ("app.wasm", wasm),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d.to_vec()))
    .collect();
    packages::pack(&files).unwrap()
}

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
    dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(MonotonicClock::new());
    let replays = ReplayLibrary::default();
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
    TestCore { app, machine, dir }
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
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn install(c: &TestCore, bytes: Vec<u8>, name: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v0/packages?name={name}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(bytes))
        .unwrap();
    let res = c.app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn latest(c: &TestCore, address: &str) -> Option<Reading> {
    match c.machine.router().subscribe(&SignalAddress::parse(address).unwrap())? {
        Subscription::Latest(rx) => rx.borrow().as_deref().cloned(),
        Subscription::Every(_) => panic!("{address} is a latest-value signal"),
    }
}

fn base(c: &TestCore) -> planar_base::Reading {
    match latest(c, "base/planar-base") {
        Some(Reading::PlanarBase(r)) => r,
        other => panic!("no base reading: {other:?}"),
    }
}

fn pose(c: &TestCore) -> (f32, f32) {
    match latest(c, "world/pose") {
        Some(Reading::Pose(p)) => (p.position.x, p.position.y),
        other => panic!("no world/pose: {other:?}"),
    }
}

/// Who holds the lease on `part`, if anyone.
fn holder(c: &TestCore, part: &str) -> Option<String> {
    c.machine.leases().into_iter().find(|l| l.part.as_str() == part).map(|l| l.holder)
}

async fn wait_for<T>(what: &str, secs: f32, mut f: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The app's state as `GET /api/v0/apps/{id}` reports it, polled until `ok` accepts it.
async fn app_state_where(c: &TestCore, id: &str, what: &str, ok: impl Fn(&Value) -> bool) -> Value {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let (s, app) = call(&c.app, "GET", &format!("/api/v0/apps/{id}"), None).await;
        assert_eq!(s, StatusCode::OK, "{app}");
        if ok(&app["state"]) {
            return app;
        }
        assert!(Instant::now() < until, "timed out waiting for {what}: {app}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The next WebSocket message `want` says yes to, within 5 s; others are skipped.
async fn next_where<S>(ws: &mut S, want: impl Fn(&Value) -> bool) -> Value
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        let m = tokio::time::timeout(left, ws.next()).await.expect("a message within 5 s").unwrap().unwrap();
        if let Message::Text(t) = m {
            let v: Value = serde_json::from_str(&t).unwrap();
            if want(&v) {
                return v;
            }
        }
    }
}

fn kind_is(kind: &'static str) -> impl Fn(&Value) -> bool {
    move |s| s["kind"] == kind
}

/// SENNEP's base and LiDAR, simulated in the apartment's living room, 0.8 m north of its south
/// wall and facing east along it.
async fn sennep_by_a_wall() -> TestCore {
    let c = core();
    let world = json!({"name": "apartment", "start": {"x": 7.0, "y": 0.8, "yaw": 0.0}});
    assert_eq!(call(&c.app, "PUT", "/api/v0/world", Some(world)).await.0, StatusCode::OK);
    for p in [
        json!({"name": "base", "connector": "generic-planar-base", "simulate": "world",
               "settings": {"length": 0.8, "width": 0.65, "max-vx": 0.6, "max-vy": 0.6, "max-omega": 1.5, "max-accel": 1.0}}),
        json!({"name": "lidar", "connector": "ldrobot-ld19", "simulate": "world", "mount": {"x": 0.2, "z": 0.3}}),
    ] {
        let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(p)).await;
        assert_eq!(s, StatusCode::CREATED, "{body}");
    }
    wait_for("the first scan and pose", 5.0, || {
        let scan = matches!(latest(&c, "lidar/laser-scan"), Some(Reading::LaserScan(_)));
        let pose = matches!(latest(&c, "world/pose"), Some(Reading::Pose(_)));
        (scan && pose).then_some(())
    })
    .await;
    c
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wall_follower_drives_along_a_wall_and_stops_when_turned_off() {
    let Some(wasm) = component("wall-follower") else { return };
    let c = sennep_by_a_wall().await;

    // A preview says what it is; installing it lists it, off and with no parts picked.
    let req = Request::builder()
        .method("POST")
        .uri("/api/v0/packages?mode=preview")
        .body(Body::from(wall_follower_package(&wasm)))
        .unwrap();
    let res = c.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let preview: Value =
        serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(preview["app"]["id"], "wall-follower", "{preview}");
    assert_eq!(
        preview["app"]["roles"][1],
        json!({"role": "base", "capability": "planar-base@1", "command": true, "count": "1", "parts": []})
    );
    let (s, installed) = install(&c, wall_follower_package(&wasm), "wall-follower-0.1.0.faberpkg").await;
    assert_eq!(s, StatusCode::CREATED, "{installed}");
    assert_eq!(installed["kind"], "app");
    let (_, apps) = call(&c.app, "GET", "/api/v0/apps", None).await;
    let a = &apps[0];
    assert_eq!((a["id"].as_str(), a["enabled"].as_bool()), (Some("wall-follower"), Some(false)), "{apps}");
    assert_eq!(a["state"]["kind"], "stopped");
    assert_eq!(a["ui"], "/apps/wall-follower/ui/index.html");
    assert_eq!(a["tickHz"], 20);
    assert_eq!(
        a["publishes"],
        json!([{"name": "wall-distance", "capability": "distance@1", "address": "app:wall-follower/wall-distance"}])
    );

    // The apps topic follows it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = c.app.clone();
    tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
    let sub = json!({"type": "subscribe", "id": 9, "topic": {"kind": "apps"}});
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();

    // On, but a role has no part: it says so, and nothing moves.
    let (s, a) = call(&c.app, "POST", "/api/v0/apps/wall-follower/enable", None).await;
    assert_eq!((s, a["enabled"].as_bool()), (StatusCode::OK, Some(true)), "{a}");
    let a = app_state_where(&c, "wall-follower", "unfilled roles", kind_is("invalid")).await;
    assert!(a["state"]["message"].as_str().unwrap().contains("role \"lidar\" has no part"), "{a}");

    // Parts are checked against the roles.
    for (roles, status, code) in [
        (json!({"lidar": "base", "base": "base"}), StatusCode::UNPROCESSABLE_ENTITY, "role-mismatch"),
        (json!({"lidar": "lidar", "camera": "lidar"}), StatusCode::BAD_REQUEST, "unknown-role"),
        (json!({"lidar": ["lidar", "base"]}), StatusCode::BAD_REQUEST, "too-many-parts"),
        (json!({"lidar": "nope"}), StatusCode::UNPROCESSABLE_ENTITY, "unknown-part"),
    ] {
        let (s, e) =
            call(&c.app, "PUT", "/api/v0/apps/wall-follower/roles", Some(json!({"roles": roles}))).await;
        assert_eq!((s, e["error"]["code"].as_str()), (status, Some(code)), "{e}");
        assert!(!e["error"]["hint"].as_str().unwrap().is_empty());
    }
    let (s, a) = call(
        &c.app,
        "PUT",
        "/api/v0/apps/wall-follower/roles",
        Some(json!({"roles": {"lidar": "lidar", "base": ["base"]}})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["roles"][0]["parts"], json!(["lidar"]));
    let apps_toml = std::fs::read_to_string(c.dir.path().join("config/apps.toml")).unwrap();
    assert!(
        apps_toml.contains("[apps.wall-follower.roles]\nbase = \"base\"\nlidar = \"lidar\""),
        "{apps_toml}"
    );
    app_state_where(&c, "wall-follower", "it runs", kind_is("running")).await;
    let mut topic_saw_running = false;
    let until = Instant::now() + Duration::from_secs(5);
    while !topic_saw_running {
        assert!(Instant::now() < until, "the apps topic never said it runs");
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        if let Message::Text(t) = m {
            let v: Value = serde_json::from_str(&t).unwrap();
            assert_eq!((v["type"].as_str(), v["id"].as_u64()), (Some("apps"), Some(9)), "{v}");
            topic_saw_running = v["apps"][0]["state"]["kind"] == "running";
        }
    }

    // It drives the base, as app:wall-follower, along the wall for five seconds without bumping.
    wait_for("the app's lease", 3.0, || {
        (holder(&c, "base").as_deref() == Some("app:wall-follower")).then_some(())
    })
    .await;
    let (x0, _) = pose(&c);
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        assert!(!base(&c).bumped, "the base bumped into something at {:?}", pose(&c));
        assert_eq!(holder(&c, "base").as_deref(), Some("app:wall-follower"), "it keeps its lease");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (x, y) = pose(&c);
    assert!(x - x0 > 0.6, "it drove east along the wall: from x {x0} to {x}");
    let centre = 0.325 + 0.3;
    assert!((y - centre).abs() < 0.08, "its side is 0.3 m from the wall: centre at y {y}, want {centre}");
    let (s, v) = call(&c.app, "GET", "/api/v0/signals/app:wall-follower/wall-distance", None).await;
    assert_eq!((s, v["capability"].as_str()), (StatusCode::OK, Some("distance@1")), "{v}");
    let gap = v["reading"]["reading"]["meters"].as_f64().unwrap_or_else(|| panic!("{v}"));
    assert!((gap - 0.3).abs() < 0.08, "the derived signal says the gap: {gap}");
    // Its window's speed slider and its logic share one state: a change through the API reaches
    // the running logic at once, every window hears it, and it is saved within a second.
    let sub = json!({"type": "subscribe", "id": 10, "topic": {"kind": "app-state", "app": "wall-follower"}});
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();
    let on_topic = |want: &'static str| move |v: &Value| v["id"] == 10 && v["type"] == want;
    let first = next_where(&mut ws, on_topic("app-state")).await;
    assert_eq!(first["values"], json!({}), "{first}");
    let (s, st) = call(
        &c.app,
        "PUT",
        "/api/v0/apps/wall-follower/state",
        Some(json!({"key": "speed", "value": "0.1"})),
    )
    .await;
    assert_eq!((s, &st["values"]), (StatusCode::OK, &json!({"speed": "0.1"})), "{st}");
    assert_eq!((st["bytes"].as_u64(), st["maxBytes"].as_u64()), (Some(8), Some(65536)));
    let changed = next_where(&mut ws, on_topic("app-state-changed")).await;
    assert_eq!((changed["key"].as_str(), changed["value"].as_str()), (Some("speed"), Some("0.1")));
    wait_for("the logic to drive at the new speed", 3.0, || {
        ((base(&c).commanded.vx - 0.1).abs() < 1e-4).then_some(())
    })
    .await;
    wait_for("the state to be saved", 3.0, || {
        std::fs::read_to_string(c.dir.path().join("apps/wall-follower/state.toml"))
            .ok()
            .filter(|t| t.contains("speed = \"0.1\""))
    })
    .await;
    let (s, e) = call(
        &c.app,
        "PUT",
        "/api/v0/apps/wall-follower/state",
        Some(json!({"key": "big", "value": "x".repeat(70_000)})),
    )
    .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("state-full")), "{e}");
    let (s, e) =
        call(&c.app, "PUT", "/api/v0/apps/wall-follower/state", Some(json!({"key": "", "value": "1"}))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("bad-key")), "{e}");
    let (s, e) = call(&c.app, "GET", "/api/v0/apps/nope/state", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("unknown-app")), "{e}");

    // Derived signals are read-only: nobody can lease or command them.
    let (s, e) =
        call(&c.app, "POST", "/api/v0/parts/app:wall-follower/lease", Some(json!({"holder": "x"}))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("unknown-part")), "{e}");

    // Its UI is served with a policy that forbids the network.
    let res = c
        .app
        .clone()
        .oneshot(Request::builder().uri("/apps/wall-follower/ui/index.html").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let h = res.headers();
    assert_eq!(h["content-type"], "text/html; charset=utf-8");
    assert!(h["content-security-policy"].to_str().unwrap().contains("connect-src 'none'"));
    assert_eq!(h["x-content-type-options"], "nosniff");
    for path in [
        "/apps/wall-follower/ui/nope.html",
        "/apps/wall-follower/ui/../faber.toml",
        "/apps/other/ui/index.html",
    ] {
        let res =
            c.app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "{path}");
    }

    // It can't be uninstalled while it is on.
    let (s, e) = call(&c.app, "DELETE", "/api/v0/packages/wall-follower", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("app-enabled")), "{e}");

    // An operator takes over: the app loses the base and waits; when they let go, it drives on.
    let (s, lease) = call(
        &c.app,
        "POST",
        "/api/v0/parts/base/lease",
        Some(json!({"holder": "Drive window", "takeOver": true})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{lease}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(holder(&c, "base").as_deref(), Some("Drive window"), "the app can't take it back");
    let id = lease["lease"].as_str().unwrap();
    let (s, _) = call(&c.app, "DELETE", &format!("/api/v0/parts/base/lease?lease={id}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    wait_for("the app to drive again", 3.0, || {
        (holder(&c, "base").as_deref() == Some("app:wall-follower")).then_some(())
    })
    .await;

    // Off: the lease ends at once and the base stops.
    let (s, a) = call(&c.app, "POST", "/api/v0/apps/wall-follower/disable", None).await;
    assert_eq!((s, a["state"]["kind"].as_str()), (StatusCode::OK, Some("stopped")), "{a}");
    assert_eq!(holder(&c, "base"), None, "turning it off ended its lease");
    assert_eq!(base(&c).commanded, faber_capabilities::Twist2d::ZERO, "and the base got its safe state");

    // On again, then a part leaves a role: it stops, saying which.
    call(&c.app, "POST", "/api/v0/apps/wall-follower/enable", None).await;
    wait_for("the app's lease", 3.0, || {
        (holder(&c, "base").as_deref() == Some("app:wall-follower")).then_some(())
    })
    .await;
    let (s, _) = call(&c.app, "DELETE", "/api/v0/parts/lidar", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let a = app_state_where(&c, "wall-follower", "the part to be missed", kind_is("conflict")).await;
    assert!(
        a["state"]["message"].as_str().unwrap().contains("\"lidar\" filling the role \"lidar\" was removed"),
        "{a}"
    );
    wait_for("its lease to end", 2.0, || holder(&c, "base").is_none().then_some(())).await;
    assert_eq!(base(&c).commanded, faber_capabilities::Twist2d::ZERO);

    // Off, it uninstalls, and its line in apps.toml goes with it.
    call(&c.app, "POST", "/api/v0/apps/wall-follower/disable", None).await;
    let (s, _) = call(&c.app, "DELETE", "/api/v0/packages/wall-follower", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(call(&c.app, "GET", "/api/v0/apps", None).await.1, json!([]));
    let apps_toml = std::fs::read_to_string(c.dir.path().join("config/apps.toml")).unwrap();
    assert!(!apps_toml.contains("wall-follower"), "{apps_toml}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trap_ends_the_lease_and_stops_the_base() {
    let Some(wasm) = component("faber-test-app") else { return };
    let c = sennep_by_a_wall().await;
    let (s, e) = install(&c, trapper_package(&wasm), "trapper.faberpkg").await;
    assert_eq!(s, StatusCode::CREATED, "{e}");
    // The debug hook: trap on the 25th tick (half a second in, at 50 Hz).
    std::fs::write(c.dir.path().join("apps/trapper/state.toml"), "trap-after = \"25\"\n").unwrap();
    call(&c.app, "PUT", "/api/v0/apps/trapper/roles", Some(json!({"roles": {"base": "base"}}))).await;
    call(&c.app, "POST", "/api/v0/apps/trapper/enable", None).await;
    wait_for("the app to drive", 3.0, || {
        (holder(&c, "base").as_deref() == Some("app:trapper") && base(&c).commanded.vx > 0.1).then_some(())
    })
    .await;
    let a = app_state_where(&c, "trapper", "the trap", kind_is("failed")).await;
    let message = a["state"]["message"].as_str().unwrap();
    assert!(message.contains("panicked (trap requested after 25 ticks)"), "{message}");
    assert_eq!(holder(&c, "base"), None, "the trap ended its lease");
    assert_eq!(base(&c).commanded, faber_capabilities::Twist2d::ZERO, "and the base stopped");

    // A limit is a crash too, and says which.
    // (While the core runs, an app's state lives in the core: it is set through the API.)
    call(&c.app, "POST", "/api/v0/apps/trapper/disable", None).await;
    for (key, value) in [("trap-after", Value::Null), ("hog", json!("yes"))] {
        let change = json!({"key": key, "value": value});
        let (s, e) = call(&c.app, "PUT", "/api/v0/apps/trapper/state", Some(change)).await;
        assert_eq!(s, StatusCode::OK, "{e}");
    }
    call(&c.app, "POST", "/api/v0/apps/trapper/enable", None).await;
    let a = app_state_where(&c, "trapper", "the memory limit", kind_is("failed")).await;
    assert!(a["state"]["message"].as_str().unwrap().contains("needed more than 32 MB"), "{a}");
    assert_eq!(holder(&c, "base"), None);
}

/// A role with `count = "0..1"` or `"0.."` may stay empty: the app runs without it, and a part
/// picked for it later is checked like any other.
#[tokio::test(flavor = "multi_thread")]
async fn an_app_runs_without_its_optional_roles() {
    let Some(wasm) = component("faber-test-app") else { return };
    let c = sennep_by_a_wall().await;
    let manifest = format!(
        "{TRAPPER}[[requires]]
role = \"pack\"
capability = \"battery@1\"
count = \"0..1\"
         [[requires]]
role = \"ring\"
capability = \"distance@1\"
count = \"0..\"
"
    );
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", manifest.as_bytes()),
        ("README.md", b"# Trapper".as_slice()),
        ("LICENSE", b"MIT".as_slice()),
        ("app.wasm", wasm.as_slice()),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d.to_vec()))
    .collect();
    let (s, e) = install(&c, packages::pack(&files).unwrap(), "trapper.faberpkg").await;
    assert_eq!(s, StatusCode::CREATED, "{e}");
    let (_, a) = call(&c.app, "GET", "/api/v0/apps/trapper", None).await;
    let counts: Vec<&str> =
        a["roles"].as_array().unwrap().iter().map(|r| r["count"].as_str().unwrap()).collect();
    assert_eq!(counts, ["1", "0..1", "0.."], "{a}");
    call(&c.app, "PUT", "/api/v0/apps/trapper/roles", Some(json!({"roles": {"base": "base"}}))).await;
    call(&c.app, "POST", "/api/v0/apps/trapper/enable", None).await;
    app_state_where(&c, "trapper", "it runs without a pack or a ring", kind_is("running")).await;

    // A 0..1 role still takes one part at most, and a 0.. role is written as a list.
    let (s, e) = call(
        &c.app,
        "PUT",
        "/api/v0/apps/trapper/roles",
        Some(json!({"roles": {"base": "base", "pack": ["lidar", "base"]}})),
    )
    .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("too-many-parts")), "{e}");
    let (s, e) = call(
        &c.app,
        "PUT",
        "/api/v0/apps/trapper/roles",
        Some(json!({"roles": {"base": "base", "ring": ["lidar"]}})),
    )
    .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("role-mismatch")),
        "{e}"
    );
    call(&c.app, "POST", "/api/v0/apps/trapper/disable", None).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connector_is_not_an_app() {
    let Some(wasm) = component("faber-test-guest") else { return };
    let c = core();
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", TRAPPER.as_bytes()),
        ("README.md", b"# Not an app".as_slice()),
        ("LICENSE", b"MIT".as_slice()),
        ("app.wasm", wasm.as_slice()),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d.to_vec()))
    .collect();
    let (s, e) = install(&c, packages::pack(&files).unwrap(), "x.faberpkg").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    assert!(
        e["error"]["message"].as_str().unwrap().contains("Trapper asks for faber:connector/context"),
        "{e}"
    );
    assert!(!c.dir.path().join("apps/trapper").exists(), "nothing installed");
}
