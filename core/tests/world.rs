//! The simulated world through the API (docs/api.md, "The simulated world").

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::runtime::Timing;
use faber_core::sim::world::Pose;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::f64::consts::PI;
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
