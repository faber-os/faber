//! Driving through the API, as a program does it (examples/drive-square.py, at test speed): take
//! the lease on a simulated base, drive a 1 m square with renewals, release it. And the stop
//! guarantees: a dropped lease stops the base within the lease time, and the e-stop stops it and
//! refuses new leases until it is released (docs/simulation.md, sections 5 and 6).

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::{Reading, Twist2d};
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::names::SignalAddress;
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::router::Subscription;
use faber_core::runtime::Timing;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
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
    let app = crate::support::signed_in_api(&machine, LogBook::new(clock));
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

/// A simulated base fast enough to drive a 1 m square in about a second.
async fn add_fast_base(c: &TestCore) {
    let part = json!({
        "name": "base",
        "connector": "generic-planar-base",
        "simulate": "manual",
        "settings": {"max-vx": 5.0, "max-vy": 5.0, "max-omega": 10.0, "max-accel": 100.0}
    });
    let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(part)).await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    assert_eq!(body["simulate"], "manual");
    assert!(body.get("port").is_none());
    // Its own limits, from its settings, not the connector's defaults.
    let info = &body["signals"][0]["info"];
    assert_eq!(info["capability"], "planar-base@1");
    assert_eq!(info["info"]["maxVelocity"]["vx"], 5.0, "{body}");
    let (_, parts) = call(&c.app, "GET", "/api/v0/parts", None).await;
    assert_eq!(parts[0]["signals"][0]["info"], *info, "the parts list says the same");
}

/// The base's latest reading: what it was commanded, and when.
fn base_state(c: &TestCore) -> (Twist2d, u64) {
    let addr = SignalAddress::parse("base/planar-base").unwrap();
    let Some(Subscription::Latest(rx)) = c.machine.router().subscribe(&addr) else {
        panic!("no base signal")
    };
    match rx.borrow().as_deref() {
        Some(Reading::PlanarBase(r)) => (r.commanded, r.t_us),
        other => panic!("not a base reading: {other:?}"),
    }
}

fn drive(vx: f32, omega: f32) -> Value {
    json!({"capability": "planar-base@1", "command": {"kind": "set-velocity", "vx": vx, "vy": 0.0, "omega": omega}})
}

async fn lease(c: &TestCore, holder: &str) -> (StatusCode, Value) {
    call(&c.app, "POST", "/api/v0/parts/base/lease", Some(json!({"holder": holder}))).await
}

async fn command(c: &TestCore, lease: &str, cmd: Value) -> (StatusCode, Value) {
    call(&c.app, "POST", "/api/v0/parts/base/command", Some(json!({"lease": lease, "command": cmd}))).await
}

/// Sends `cmd` every 50 ms for `secs`, as a program would; every command renews the lease.
async fn hold(c: &TestCore, lease: &str, cmd: Value, secs: f32) {
    let until = Instant::now() + Duration::from_secs_f32(secs);
    while Instant::now() < until {
        let (s, body) = command(c, lease, cmd.clone()).await;
        assert_eq!(s, StatusCode::OK, "{body}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_drives_a_square_and_lets_go() {
    let c = core();
    add_fast_base(&c).await;
    let (s, grant) = lease(&c, "drive-square test").await;
    assert_eq!(s, StatusCode::CREATED, "{grant}");
    assert_eq!(grant["durationMs"], 250);
    let id = grant["lease"].as_str().unwrap().to_string();

    let (s, held) = lease(&c, "someone else").await;
    assert_eq!((s, held["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("lease-held")));
    assert!(held["error"]["message"].as_str().unwrap().contains("drive-square test"));

    let (_, list) = call(&c.app, "GET", "/api/v0/leases", None).await;
    assert_eq!(list[0]["holder"], "drive-square test");
    assert!(list[0].get("lease").is_none(), "the id is the holder's secret");

    for _ in 0..4 {
        hold(&c, &id, drive(5.0, 0.0), 0.2).await; // 1 m
        assert_eq!(base_state(&c).0, Twist2d::new(5.0, 0.0, 0.0));
        hold(&c, &id, drive(0.0, 10.0), 0.157).await; // a quarter turn
        assert_eq!(base_state(&c).0, Twist2d::new(0.0, 0.0, 10.0));
    }

    let (s, reply) = command(&c, &id, drive(9.0, -30.0)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(reply["applied"], drive(5.0, -10.0), "clamped to the base's limits");
    assert_eq!(reply["clamped"], true);

    let (s, renewed) =
        call(&c.app, "POST", "/api/v0/parts/base/lease/renew", Some(json!({"lease": id}))).await;
    assert_eq!(s, StatusCode::OK);
    assert!(renewed["deadlineUs"].as_u64() > grant["deadlineUs"].as_u64());

    let (s, _) = call(&c.app, "DELETE", &format!("/api/v0/parts/base/lease?lease={id}"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(base_state(&c).0, Twist2d::ZERO, "releasing the lease stops the base");
    let (s, e) = command(&c, &id, drive(0.1, 0.0)).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("lease-ended")));
    assert_eq!(e["error"]["reason"], "released", "why it ended, without reading the message");
    let (s, e) = command(&c, "made-up", drive(0.1, 0.0)).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("lease-required")));
    assert!(e["error"].get("reason").is_none(), "only lease-ended has a reason: {e}");
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_lease_stops_the_base_within_the_lease_time() {
    let c = core();
    add_fast_base(&c).await;
    let (_, grant) = lease(&c, "a program that crashes").await;
    let id = grant["lease"].as_str().unwrap();
    let (_, reply) = command(&c, id, drive(1.0, 0.0)).await;
    let deadline = reply["deadlineUs"].as_u64().unwrap();
    // The program goes quiet. Nothing else is sent.
    let start = Instant::now();
    let stopped_at = loop {
        let (v, t) = base_state(&c);
        if v == Twist2d::ZERO {
            break t;
        }
        assert!(start.elapsed() < Duration::from_secs(2), "the base never stopped");
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    assert!(stopped_at >= deadline, "not before the lease ran out");
    assert!(
        stopped_at <= deadline + 100_000,
        "within one sweep (50 ms) and some scheduling: {}",
        stopped_at - deadline
    );
    let (s, e) = command(&c, id, drive(1.0, 0.0)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert!(e["error"]["message"].as_str().unwrap().contains("expired"), "{e}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_estop_stops_the_base_and_refuses_leases_until_released() {
    let c = core();
    add_fast_base(&c).await;
    let (_, grant) = lease(&c, "driver").await;
    let id = grant["lease"].as_str().unwrap();
    command(&c, id, drive(1.0, 0.0)).await;

    let (s, status) =
        call(&c.app, "POST", "/api/v0/estop", Some(json!({"reason": "Cat on the track", "source": "shell"})))
            .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(status["engaged"], true);
    assert_eq!(status["engagedBy"]["reason"], "Cat on the track");
    assert_eq!(base_state(&c).0, Twist2d::ZERO, "stopped at once");

    let (s, e) = command(&c, id, drive(1.0, 0.0)).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::LOCKED, Some("estop-engaged")));
    let (s, e) = lease(&c, "driver").await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::LOCKED, Some("estop-engaged")));
    assert!(e["error"]["message"].as_str().unwrap().contains("Cat on the track"));

    // Engaging never fails, not even with an empty or unreadable body.
    let req = Request::post("/api/v0/estop").body(Body::from("{oops")).unwrap();
    assert_eq!(c.app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
    let (s, _) = call(&c.app, "POST", "/api/v0/estop", None).await;
    assert_eq!(s, StatusCode::OK);

    let (s, status) = call(&c.app, "POST", "/api/v0/estop/release", None).await;
    assert_eq!((s, &status["engaged"]), (StatusCode::OK, &json!(false)));
    let (_, now) = call(&c.app, "GET", "/api/v0/estop", None).await;
    assert_eq!(now["history"].as_array().unwrap().len(), 2, "{now}");
    let (s, _) = lease(&c, "driver").await;
    assert_eq!(s, StatusCode::CREATED, "leases work again after release");
}

#[tokio::test(flavor = "multi_thread")]
async fn parts_and_patterns_through_the_api() {
    let c = core();
    let (s, e) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "base", "connector": "generic-planar-base"})),
    )
    .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("needs-bridge")));
    let front = json!({"name": "front", "connector": "generic-distance", "simulate": "manual", "settings": {"max-m": 2}});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(front)).await.0, StatusCode::CREATED);
    let pattern = json!({"pattern": {"kind": "sine", "center": 1.0, "amplitude": 0.5, "periodS": 4.0}});
    let (s, body) = call(&c.app, "PUT", "/api/v0/parts/front/simulation", Some(pattern.clone())).await;
    assert_eq!((s, &body), (StatusCode::OK, &pattern));
    assert_eq!(call(&c.app, "GET", "/api/v0/parts/front/simulation", None).await.1, pattern);
    let bad = json!({"pattern": {"kind": "step", "low": 0, "high": 1, "periodS": 0}});
    let (s, e) = call(&c.app, "PUT", "/api/v0/parts/front/simulation", Some(bad)).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("bad-pattern")));
    let (s, e) = call(&c.app, "POST", "/api/v0/parts/front/lease", Some(json!({"holder": "x"}))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("not-leasable")));
    let (_, connectors) = call(&c.app, "GET", "/api/v0/connectors", None).await;
    let generic = connectors.as_array().unwrap().iter().find(|c| c["id"] == "generic-distance").unwrap();
    assert_eq!(generic["port"], json!({"kind": "bridge"}));
    assert_eq!(generic["provides"][0]["info"]["capability"], "distance@1");
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
async fn every_screen_hears_who_drives_and_the_estop() {
    let c = core();
    add_fast_base(&c).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = c.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
    for (id, kind) in [(1, "leases"), (2, "estop")] {
        let msg = json!({"type": "subscribe", "id": id, "topic": {"kind": kind}});
        ws.send(Message::Text(msg.to_string().into())).await.unwrap();
    }
    let mut first = [next(&mut ws).await, next(&mut ws).await];
    first.sort_by_key(|m| m["id"].as_u64());
    assert_eq!(first[0], json!({"type": "leases", "id": 1, "leases": []}));
    assert_eq!(first[1], json!({"type": "estop", "id": 2, "status": {"engaged": false}}));

    let (_, grant) = lease(&c, "Drive window").await;
    let granted = next(&mut ws).await;
    assert_eq!(
        (granted["type"].as_str(), granted["lease"]["holder"].as_str()),
        (Some("lease-granted"), Some("Drive window"))
    );
    assert!(granted["lease"].get("lease").is_none(), "nobody else learns the id");

    call(&c.app, "POST", "/api/v0/estop", Some(json!({"reason": "test"}))).await;
    let mut got = [next(&mut ws).await, next(&mut ws).await];
    got.sort_by_key(|m| m["id"].as_u64());
    assert_eq!((got[0]["type"].as_str(), got[0]["reason"].as_str()), (Some("lease-ended"), Some("estop")));
    assert_eq!(got[0]["lease"]["part"], "base");
    assert_eq!(got[1]["status"]["engaged"], true);
    assert_eq!(got[1]["status"]["engagedBy"]["source"], "app");
    assert!(grant["lease"].is_string());

    call(&c.app, "POST", "/api/v0/estop/release", Some(json!({"source": "shell"}))).await;
    let released = next(&mut ws).await;
    assert_eq!(released, json!({"type": "estop", "id": 2, "status": {"engaged": false}}));
}

/// docs/capabilities.md, battery@1: "The core raises a notification whenever level changes".
#[tokio::test(flavor = "multi_thread")]
async fn every_screen_hears_when_a_batterys_level_changes() {
    let c = core();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = c.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
    let msg = json!({"type": "subscribe", "id": 3, "topic": {"kind": "notices"}});
    ws.send(Message::Text(msg.to_string().into())).await.unwrap();
    let pack = json!({"name": "pack", "connector": "generic-battery", "simulate": "manual", "settings": {"rate-hz": 50}});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(pack)).await.0, StatusCode::CREATED);
    // The first reading (ok) must be published before the level drops, or the drop has no
    // "previous" level: on a slow machine the new pattern can win the race.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let (_, v) = call(&c.app, "GET", "/api/v0/signals/pack/battery", None).await;
        if v["reading"]["reading"]["level"] == "ok" {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no first reading: {v}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let low = json!({"pattern": {"kind": "steady", "value": 10.2}});
    assert_eq!(call(&c.app, "PUT", "/api/v0/parts/pack/simulation", Some(low)).await.0, StatusCode::OK);
    let notice = next(&mut ws).await;
    assert_eq!((notice["type"].as_str(), notice["id"].as_u64()), (Some("notice"), Some(3)));
    let n = &notice["notice"];
    assert_eq!(
        (n["kind"].as_str(), n["address"].as_str(), n["level"].as_str(), n["previous"].as_str()),
        (Some("battery-level"), Some("pack/battery"), Some("low"), Some("ok")),
        "{notice}"
    );
    assert!((n["volts"].as_f64().unwrap() - 10.2).abs() < 1e-4);
    let ok = json!({"pattern": {"kind": "steady", "value": 12.0}});
    call(&c.app, "PUT", "/api/v0/parts/pack/simulation", Some(ok)).await;
    let back = next(&mut ws).await;
    assert_eq!(
        (back["notice"]["level"].as_str(), back["notice"]["previous"].as_str()),
        (Some("ok"), Some("low"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_can_read_the_latest_value_of_a_signal() {
    let c = core();
    let (s, e) = call(&c.app, "GET", "/api/v0/signals/front/distance", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("unknown-signal")));
    let (s, _) = call(&c.app, "GET", "/api/v0/signals/not%20a%20name/x", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let front = json!({"name": "front", "connector": "generic-distance", "simulate": "manual", "settings": {"max-m": 2}});
    assert_eq!(call(&c.app, "POST", "/api/v0/parts", Some(front)).await.0, StatusCode::CREATED);
    let steady = json!({"pattern": {"kind": "steady", "value": 1.25}});
    assert_eq!(call(&c.app, "PUT", "/api/v0/parts/front/simulation", Some(steady)).await.0, StatusCode::OK);

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let (s, v) = call(&c.app, "GET", "/api/v0/signals/front/distance", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(
            (v["address"].as_str(), v["capability"].as_str()),
            (Some("front/distance"), Some("distance@1"))
        );
        // Readings from before the new pattern may still be the newest for a moment.
        if v["reading"]["reading"]["meters"].as_f64().is_some_and(|m| (m - 1.25).abs() < 1e-6) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no reading arrived: {v}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
