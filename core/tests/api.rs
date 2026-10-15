//! The whole core, end to end: the built-in LD19 recording goes through a replay port, the
//! connector, the router and the WebSocket, exactly as the shell uses it (docs/api.md).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::Reading;
use faber_core::api::{self, dto};
use faber_core::clock::MonotonicClock;
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{Discovered, ReplayLibrary, SerialDiscovery, SystemOpener};
use faber_core::runtime::Timing;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

struct NoSerialPorts;
impl SerialDiscovery for NoSerialPorts {
    fn list(&self) -> Vec<Discovered> {
        Vec::new()
    }
}

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    _dir: tempfile::TempDir,
}

fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(MonotonicClock::new());
    let replays = ReplayLibrary::with_demo();
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(NoSerialPorts),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: Timing::default(),
        port_scan: None,
    })
    .unwrap();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, _dir: dir }
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

async fn call(
    app: &axum::Router,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
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
    let value =
        if bytes.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, value)
}

fn demo_part(name: &str) -> serde_json::Value {
    json!({"name": name, "connector": "ldrobot-ld19", "port": "replay:demo-ld19"})
}

#[tokio::test(flavor = "multi_thread")]
async fn rest_lists_the_machine_and_manages_parts() {
    let c = core();
    let (s, system) = call(&c.app, "GET", "/api/v0/system", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(system["name"], "Faber");
    assert!(system.get("configProblem").is_none());

    let (_, connectors) = call(&c.app, "GET", "/api/v0/connectors", None).await;
    assert_eq!(connectors[0]["id"], "ldrobot-ld19");
    assert_eq!(connectors[0]["port"], json!({"kind": "serial", "baud": 230400}));

    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    let demo = ports.as_array().unwrap().iter().find(|p| p["id"] == "replay:demo-ld19").unwrap();
    assert_eq!(demo["kind"], "replay");
    assert_eq!(demo["suggestions"][0]["connector"], "ldrobot-ld19");

    let (s, part) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CREATED, "{part}");
    assert_eq!(part["signals"][0], json!({"address": "lidar/laser-scan", "capability": "laser-scan@1"}));

    let (s, err) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "name-taken");
    assert!(err["error"]["hint"].as_str().unwrap().len() > 5);

    let (s, err) = call(&c.app, "POST", "/api/v0/parts", Some(json!({"name": 5}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "bad-request", "bad bodies get an ApiError too");

    let (_, parts) = call(&c.app, "GET", "/api/v0/parts", None).await;
    assert_eq!(parts[0]["name"], "lidar");

    let (s, _) = call(&c.app, "DELETE", "/api/v0/parts/lidar", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, err) = call(&c.app, "DELETE", "/api/v0/parts/lidar", None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(err["error"]["code"], "unknown-part");
}

async fn serve(app: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(addr: std::net::SocketAddr, format: &str) -> Ws {
    let (ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format={format}")).await.unwrap();
    ws
}

async fn send(ws: &mut Ws, msg: &dto::ClientMessage) {
    ws.send(Message::Binary(rmp_serde::to_vec_named(msg).unwrap().into())).await.unwrap();
}

/// The next message from the core that matches `want`, within 5 s.
async fn wait_for(ws: &mut Ws, want: impl Fn(&dto::ServerMessage) -> bool) -> dto::ServerMessage {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let msg = ws.next().await.expect("socket open").unwrap();
            let decoded: dto::ServerMessage = match &msg {
                Message::Binary(b) => rmp_serde::from_slice(b).unwrap(),
                Message::Text(t) => serde_json::from_str(t).unwrap(),
                _ => continue,
            };
            if want(&decoded) {
                return decoded;
            }
        }
    })
    .await
    .expect("expected message within 5 s")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_ld19_recording_streams_whole_revolutions_over_the_websocket() {
    let c = core();
    let (s, _) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CREATED);
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 1, topic: dto::Topic::Parts }).await;
    let dto::ServerMessage::Parts { parts, .. } =
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Parts { parts, .. } if parts[0].state.kind == dto::PartStateKind::Running)).await
    else {
        unreachable!()
    };
    assert_eq!(parts[0].name, "lidar");

    let topic = dto::Topic::Signal { address: "lidar/laser-scan".into(), max_hz: Some(15.0) };
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 2, topic }).await;
    for _ in 0..3 {
        let dto::ServerMessage::Signal { id, address, reading: Reading::LaserScan(scan) } =
            wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Signal { .. })).await
        else {
            unreachable!()
        };
        assert_eq!((id, address.as_str()), (2, "lidar/laser-scan"));
        scan.validate().unwrap();
        assert!((400..=520).contains(&scan.len()), "{} points", scan.len());
    }
}

/// JSON debug mode delivers the same readings. (That `+inf` becomes `null` in JSON is pinned by
/// a unit test in faber-capabilities: only ~4 % of the recording's revolutions have a no-return
/// point, so this test can't rely on seeing one.)
#[tokio::test(flavor = "multi_thread")]
async fn json_mode_delivers_readable_scans() {
    let c = core();
    call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "json").await;
    let sub =
        json!({"type": "subscribe", "id": 1, "topic": {"kind": "signal", "address": "lidar/laser-scan"}});
    ws.send(Message::Text(sub.to_string().into())).await.unwrap();
    let raw = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "signal" {
                    return v;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(raw["reading"]["capability"], "laser-scan@1");
    let ranges = raw["reading"]["reading"]["ranges"].as_array().unwrap();
    assert!(ranges.len() >= 400);
    assert!(ranges.iter().all(|r| r.is_f64() || r.is_null()), "numbers, or null for no return");
    let msg: dto::ServerMessage = serde_json::from_value(raw).unwrap();
    assert!(matches!(msg, dto::ServerMessage::Signal { .. }), "JSON decodes into the same types");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_serial_monitor_reads_a_port_and_refuses_one_in_use() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    send(&mut ws, &dto::ClientMessage::SerialOpen { id: 9, port: "replay:demo-ld19".into(), baud: 230_400 })
        .await;
    let dto::ServerMessage::SerialData { data, .. } =
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::SerialData { .. })).await
    else {
        unreachable!()
    };
    assert!(!data.is_empty());

    let (s, err) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "port-claimed");

    send(&mut ws, &dto::ClientMessage::SerialClose { id: 9 }).await;
    wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::SerialClosed { id: 9, .. })).await;
    let (s, _) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CREATED, "closing the monitor frees the port");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_signals_and_reused_ids_get_actionable_errors() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    let topic = dto::Topic::Signal { address: "nope/laser-scan".into(), max_hz: None };
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 1, topic }).await;
    let dto::ServerMessage::Error { id, error } =
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Error { .. })).await
    else {
        unreachable!()
    };
    assert_eq!((id, error.code.as_str()), (Some(1), "unknown-signal"));
    assert!(error.hint.contains("part/name"));

    send(&mut ws, &dto::ClientMessage::Subscribe { id: 2, topic: dto::Topic::Logs }).await;
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 2, topic: dto::Topic::Ports }).await;
    let dto::ServerMessage::Error { error, .. } =
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Error { id: Some(2), .. })).await
    else {
        unreachable!()
    };
    assert_eq!(error.code, "id-in-use");
}
