//! The whole core, end to end: the built-in LD19 recording goes through a replay port, the
//! connector, the router and the WebSocket, exactly as the shell uses it (docs/api.md).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_capabilities::Reading;
use faber_core::api::{self, dto};
use faber_core::clock::MonotonicClock;
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{
    Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
};
use faber_core::runtime::Timing;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

/// Serial ports the test can plug in and out.
#[derive(Default)]
struct FakeSerialPorts(std::sync::Mutex<Vec<Discovered>>);
impl SerialDiscovery for FakeSerialPorts {
    fn list(&self) -> Vec<Discovered> {
        self.0.lock().unwrap().clone()
    }
}

/// USB things the computer can't talk to, which the test can plug in and out.
#[derive(Default)]
struct FakeUsbProblems(std::sync::Mutex<Vec<FailedUsb>>);
impl UsbProblemDiscovery for FakeUsbProblems {
    fn list(&self) -> Vec<FailedUsb> {
        self.0.lock().unwrap().clone()
    }
}

/// Removable disks the test can plug in and out.
#[derive(Default)]
struct FakeDisks(std::sync::Mutex<Vec<Disk>>);
impl DiskDiscovery for FakeDisks {
    fn list(&self) -> Vec<Disk> {
        self.0.lock().unwrap().clone()
    }
}

/// What Windows 11 reports for a cable it can't talk to (seen on a long passive extension).
fn port_reset_failed() -> FailedUsb {
    FailedUsb {
        os_id: r"USB\VID_0000&PID_0001\5&DAB2676&0&5".into(),
        hardware_ids: vec![r"USB\RESET_FAILURE".into()],
        problem_code: Some(43),
    }
}

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    serial: Arc<FakeSerialPorts>,
    usb_problems: Arc<FakeUsbProblems>,
    disks: Arc<FakeDisks>,
    _dir: tempfile::TempDir,
}

fn core() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(MonotonicClock::new());
    let replays = ReplayLibrary::with_demo();
    let serial = Arc::new(FakeSerialPorts::default());
    let usb_problems = Arc::new(FakeUsbProblems::default());
    let disks = Arc::new(FakeDisks::default());
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::clone(&serial) as Arc<dyn SerialDiscovery>,
        usb_problems: Arc::clone(&usb_problems) as Arc<dyn UsbProblemDiscovery>,
        disks: Arc::clone(&disks) as Arc<dyn DiskDiscovery>,
        opener: Arc::new(SystemOpener::new(replays.clone())),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: Timing::default(),
        port_scan: None,
    })
    .unwrap();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, serial, usb_problems, disks, _dir: dir }
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

/// Waits (up to 5 s) until the part `name` is running, so its signals exist.
async fn wait_until_running(app: &axum::Router, name: &str) {
    let start = std::time::Instant::now();
    loop {
        let (_, parts) = call(app, "GET", "/api/v0/parts", None).await;
        let running =
            parts.as_array().unwrap().iter().any(|p| p["name"] == name && p["state"]["kind"] == "running");
        if running {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "{name} never started: {parts}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
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
    // A signal only exists once its part runs; subscribing earlier is `unknown-signal`.
    wait_until_running(&c.app, "lidar").await;
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
    let first = wait_for(&mut ws, |m| {
        matches!(m, dto::ServerMessage::SerialOpened { .. } | dto::ServerMessage::SerialData { .. })
    })
    .await;
    assert_eq!(
        first,
        dto::ServerMessage::SerialOpened { id: 9, port: "replay:demo-ld19".into() },
        "the session is announced before any data"
    );
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

#[tokio::test(flavor = "multi_thread")]
async fn the_ports_topic_pushes_a_newly_plugged_adapter() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 1, topic: dto::Topic::Ports }).await;
    wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Ports { .. })).await;
    c.serial.0.lock().unwrap().push(Discovered {
        path: "COM5".into(),
        bluetooth: false,
        usb: Some(faber_core::ports::Usb {
            vid: 0x10c4,
            pid: 0xea60,
            serial_number: Some("0001".into()),
            manufacturer: Some("Silicon Labs".into()),
            product: Some("CP2102 USB to UART Bridge".into()),
        }),
    });
    let machine = Arc::clone(&c.machine);
    tokio::task::spawn_blocking(move || machine.refresh_ports()).await.unwrap();
    let dto::ServerMessage::Ports { ports, .. } = wait_for(&mut ws, |m| {
        matches!(m, dto::ServerMessage::Ports { ports, .. } if ports.iter().any(|p| p.id == "usb:10c4:ea60"))
    })
    .await
    else {
        unreachable!()
    };
    let cp2102 = ports.iter().find(|p| p.id == "usb:10c4:ea60").unwrap();
    assert_eq!(cp2102.label, "CP2102 USB to UART Bridge (COM5)");
    assert_eq!(cp2102.suggestions[0].connector, "ldrobot-ld19");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ports_topic_says_when_something_is_plugged_in_but_cant_be_talked_to() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 1, topic: dto::Topic::Ports }).await;
    let dto::ServerMessage::Ports { problems, .. } =
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Ports { .. })).await
    else {
        unreachable!()
    };
    assert!(problems.is_empty());

    c.usb_problems.0.lock().unwrap().push(port_reset_failed());
    let machine = Arc::clone(&c.machine);
    tokio::task::spawn_blocking(move || machine.refresh_ports()).await.unwrap();
    let dto::ServerMessage::Ports { problems, .. } = wait_for(
        &mut ws,
        |m| matches!(m, dto::ServerMessage::Ports { problems, .. } if !problems.is_empty()),
    )
    .await
    else {
        unreachable!()
    };
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, "usb-no-answer");
    assert_eq!(problems[0].label, "Plugged in, but the computer can't talk to it");
    assert!(problems[0].hint.contains("cable"));

    c.usb_problems.0.lock().unwrap().clear();
    let machine = Arc::clone(&c.machine);
    tokio::task::spawn_blocking(move || machine.refresh_ports()).await.unwrap();
    wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::Ports { problems, .. } if problems.is_empty()))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn port_problems_are_left_out_of_the_json_when_there_are_none() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "json").await;
    let subscribe = json!({ "type": "subscribe", "id": 1, "topic": { "kind": "ports" } });
    ws.send(Message::text(subscribe.to_string())).await.unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(Ok(Message::Text(t))) = ws.next().await {
                let v: serde_json::Value = serde_json::from_str(&t).unwrap();
                if v["type"] == "ports" {
                    return v;
                }
            }
        }
    })
    .await
    .expect("a ports message within 5 s");
    assert!(first.get("problems").is_none(), "existing clients see the old shape: {first}");
}

#[tokio::test]
async fn port_problems_have_their_own_endpoint_and_leave_the_port_list_alone() {
    let c = core();
    let (s, problems) = call(&c.app, "GET", "/api/v0/ports/problems", None).await;
    assert_eq!((s, problems), (StatusCode::OK, json!([])));

    c.usb_problems.0.lock().unwrap().push(port_reset_failed());
    let machine = Arc::clone(&c.machine);
    tokio::task::spawn_blocking(move || machine.refresh_ports()).await.unwrap();
    let (s, problems) = call(&c.app, "GET", "/api/v0/ports/problems", None).await;
    assert_eq!(s, StatusCode::OK);
    let p = &problems[0];
    assert_eq!(p["code"], "usb-no-answer");
    assert_eq!(p["label"], "Plugged in, but the computer can't talk to it");
    assert!(p["detail"].as_str().unwrap().contains("didn't answer"));
    assert!(p["hint"].as_str().is_some_and(|h| !h.is_empty()));

    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert!(ports.is_array(), "GET /api/v0/ports still returns PortInfo[]");
}

async fn put_bytes(app: &axum::Router, path: &str, bytes: &'static [u8]) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("PUT")
        .uri(format!("/api/v0/files/content?path={path}"))
        .body(Body::from(bytes))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, if body.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&body).unwrap() })
}

async fn get_bytes(app: &axum::Router, path: &str) -> (StatusCode, Vec<u8>) {
    let req =
        Request::builder().uri(format!("/api/v0/files/content?path={path}")).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    (status, axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap().to_vec())
}

#[tokio::test(flavor = "multi_thread")]
async fn files_round_trip_through_the_api() {
    let c = core();
    let (s, roots) = call(&c.app, "GET", "/api/v0/files/roots", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(roots, json!([{"id": "home", "label": "Home", "path": "home:/"}]));

    let (_, home) = call(&c.app, "GET", "/api/v0/files/list?path=home:/", None).await;
    let names: Vec<_> =
        home["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Desktop", "Documents", "Recordings", "Backups", "System", "Trash"]);
    assert_eq!(home["entries"][4]["special"], "system");

    let (s, _) =
        call(&c.app, "POST", "/api/v0/files/folder", Some(json!({"path": "home:/Documents/robot"}))).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, _) = put_bytes(&c.app, "home:/Documents/robot/notes.txt", b"wheel torque").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        get_bytes(&c.app, "home:/Documents/robot/notes.txt").await,
        (StatusCode::OK, b"wheel torque".to_vec())
    );

    let (s, _) = call(
        &c.app,
        "POST",
        "/api/v0/files/move",
        Some(json!({"from": "home:/Documents/robot/notes.txt", "to": "home:/Desktop/notes.txt"})),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) =
        call(&c.app, "POST", "/api/v0/files/trash", Some(json!({"paths": ["home:/Desktop/notes.txt"]})))
            .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, trash) = call(&c.app, "GET", "/api/v0/files/list?path=home:/Trash", None).await;
    assert_eq!(trash["entries"][0]["originalPath"], "home:/Desktop/notes.txt");
    assert_eq!(trash["parent"], "home:/");

    let (s, err) =
        call(&c.app, "POST", "/api/v0/files/delete", Some(json!({"paths": ["home:/Documents/robot"]}))).await;
    assert_eq!(s, StatusCode::CONFLICT, "deleting for good only works from the Trash");
    assert_eq!(err["error"]["code"], "protected");

    let (s, err) = call(&c.app, "GET", "/api/v0/files/list?path=home:/../secret", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "bad-path");
    let (s, err) = call(&c.app, "GET", "/api/v0/files/list?path=machine:/", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(err["error"]["code"], "machine-files-off");
}

#[tokio::test(flavor = "multi_thread")]
async fn parts_toml_is_validated_when_saved_from_files() {
    let c = core();
    let (s, err) = put_bytes(&c.app, "home:/System/parts.toml", b"[parts.lidar]\nconnector = \n").await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "config-invalid");
    assert!(err["error"]["message"].as_str().unwrap().contains("line 2"));
    let (s, _) = put_bytes(
        &c.app,
        "home:/System/parts.toml",
        b"[parts.lidar]\nconnector = \"ldrobot-ld19\"\nport = \"replay:demo-ld19\"\n",
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, parts) = call(&c.app, "GET", "/api/v0/parts", None).await;
    assert_eq!(parts[0]["name"], "lidar", "the saved config is applied at once");
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_and_the_developer_guard() {
    let c = core();
    let (_, mut settings) = call(&c.app, "GET", "/api/v0/settings", None).await;
    assert_eq!(
        settings["appearance"],
        json!({"theme": "light", "style": "aluminium", "wallpaper": "aluminium"})
    );
    assert_eq!(settings["taskbar"]["size"], "small");
    settings["appearance"]["theme"] = json!("dark");
    settings["desktop"]["icons"] = json!([{"key": "accessory:files", "x": 16.0, "y": 16.0}]);
    let (s, saved) = call(&c.app, "PUT", "/api/v0/settings", Some(settings.clone())).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(saved["appearance"]["theme"], "dark");

    settings["developer"]["fullMachineFiles"] = json!(true);
    let (s, err) = call(&c.app, "PUT", "/api/v0/settings", Some(settings)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "use-developer-endpoint");

    let (s, err) =
        call(&c.app, "POST", "/api/v0/settings/developer", Some(json!({"fullMachineFiles": true}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "risk-not-acknowledged");
    let (s, on) = call(
        &c.app,
        "POST",
        "/api/v0/settings/developer",
        Some(json!({"fullMachineFiles": true, "acknowledgedRisk": true})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(on["developer"]["fullMachineFiles"], true);
    let (_, roots) = call(&c.app, "GET", "/api/v0/files/roots", None).await;
    assert!(roots[1]["warning"].as_str().unwrap().contains("break"));
}

#[tokio::test(flavor = "multi_thread")]
async fn recording_from_the_serial_monitor_makes_a_replay_port() {
    let c = core();
    let addr = serve(c.app.clone()).await;
    let mut ws = connect(addr, "msgpack").await;
    send(&mut ws, &dto::ClientMessage::Subscribe { id: 1, topic: dto::Topic::Files }).await;
    send(&mut ws, &dto::ClientMessage::SerialOpen { id: 2, port: "replay:demo-ld19".into(), baud: 230_400 })
        .await;
    wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::SerialOpened { .. })).await;
    send(&mut ws, &dto::ClientMessage::SerialRecordStart { id: 2, name: "bench".into() }).await;
    let rec = wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::SerialRecording { .. })).await;
    assert_eq!(
        rec,
        dto::ServerMessage::SerialRecording { id: 2, path: "home:/Recordings/bench.fbcap".into() }
    );
    for _ in 0..3 {
        wait_for(&mut ws, |m| matches!(m, dto::ServerMessage::SerialData { .. })).await;
    }
    send(&mut ws, &dto::ClientMessage::SerialClose { id: 2 }).await;
    // Both arrive, in either order: the stop report and the Recordings folder change.
    let (mut stopped, mut announced) = (None, false);
    while stopped.is_none() || !announced {
        match wait_for(&mut ws, |m| {
            matches!(
                m,
                dto::ServerMessage::SerialRecordingStopped { .. } | dto::ServerMessage::FilesChanged { .. }
            )
        })
        .await
        {
            dto::ServerMessage::SerialRecordingStopped { path, bytes, .. } => stopped = Some((path, bytes)),
            dto::ServerMessage::FilesChanged { folders, .. } => {
                announced |= folders.contains(&"home:/Recordings".to_string())
            }
            _ => unreachable!(),
        }
    }
    let (path, bytes) = stopped.unwrap();
    assert_eq!(path, "home:/Recordings/bench.fbcap");
    assert!(bytes > 0);
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert!(ports.as_array().unwrap().iter().any(|p| p["id"] == "replay:bench"));
}

#[tokio::test(flavor = "multi_thread")]
async fn uploads_can_refuse_to_overwrite() {
    let c = core();
    let (s, _) = put_bytes(&c.app, "home:/Documents/a.txt&mode=create", b"one").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, err) = put_bytes(&c.app, "home:/Documents/a.txt&mode=create", b"two").await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(err["error"]["code"], "already-exists");
    assert_eq!(get_bytes(&c.app, "home:/Documents/a.txt").await.1, b"one");
    let (s, _) = put_bytes(&c.app, "home:/Documents/a.txt", b"two").await;
    assert_eq!(s, StatusCode::NO_CONTENT, "without a mode, saving replaces");
    let (s, err) = put_bytes(&c.app, "home:/Documents/a.txt&mode=sideways", b"x").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "bad-request");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_usb_stick_shows_up_as_its_own_place() {
    let c = core();
    let stick = tempfile::tempdir().unwrap();
    std::fs::write(stick.path().join("notes.txt"), "on the stick").unwrap();
    c.disks.0.lock().unwrap().push(Disk {
        id: "disk-e".into(),
        label: "STICK (E:)".into(),
        mount: stick.path().to_path_buf(),
    });
    c.machine.refresh_ports();
    let (_, roots) = call(&c.app, "GET", "/api/v0/files/roots", None).await;
    assert_eq!(roots[1], json!({"id": "disk-e", "label": "STICK (E:)", "path": "disk-e:/"}));
    let (s, list) = call(&c.app, "GET", "/api/v0/files/list?path=disk-e:/", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(list["entries"][0]["name"], "notes.txt");
    let (s, body) =
        call(&c.app, "POST", "/api/v0/files/trash", Some(json!({"paths": ["disk-e:/notes.txt"]}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("no-trash")));

    c.disks.0.lock().unwrap().clear();
    c.machine.refresh_ports();
    let (s, body) = call(&c.app, "GET", "/api/v0/files/list?path=disk-e:/", None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not-found")));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_system_about_and_storage_describe_this_machine() {
    let c = core();
    let (_, system) = call(&c.app, "GET", "/api/v0/system", None).await;
    assert!(system["timeZone"].as_str().is_some_and(|z| !z.is_empty()), "{system}");

    let (s, about) = call(&c.app, "GET", "/api/v0/about", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(about["version"], system["version"]);
    assert_eq!(about["api"], system["api"]);
    assert_eq!(about["timeZone"], system["timeZone"]);
    assert_eq!(about["dataFolder"], c._dir.path().display().to_string());
    assert!(about["uptimeS"].is_u64());
    assert!(about["os"].as_str().is_some_and(|o| !o.is_empty()));

    put_bytes(&c.app, "home:/Documents/a.txt", b"12345").await;
    let (s, storage) = call(&c.app, "GET", "/api/v0/storage", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(storage["documentsBytes"], 5);
    assert_eq!(storage["recordingsBytes"], 0);
    assert_eq!(storage["desktopBytes"], 0);
    assert_eq!(storage["backupsBytes"], 0);
    assert!(storage["freeBytes"].as_u64().unwrap() <= storage["totalBytes"].as_u64().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn backups_are_made_and_restored_through_the_api() {
    let c = core();
    let (s, _) = call(&c.app, "POST", "/api/v0/parts", Some(demo_part("lidar"))).await;
    assert_eq!(s, StatusCode::CREATED);
    put_bytes(&c.app, "home:/Documents/notes.txt", b"v1").await;
    let (s, made) = call(&c.app, "POST", "/api/v0/backups", Some(json!({"includeRecordings": false}))).await;
    assert_eq!(s, StatusCode::CREATED);
    let path = made["path"].as_str().unwrap().to_string();
    assert!(path.starts_with("home:/Backups/faber-backup-"), "{made}");
    assert!(made["bytes"].as_u64().unwrap() > 0);
    let (_, backups) = call(&c.app, "GET", "/api/v0/files/list?path=home:/Backups", None).await;
    assert_eq!(backups["entries"][0]["path"], path.as_str());
    let (_, home) = call(&c.app, "GET", "/api/v0/files/list?path=home:/", None).await;
    assert_eq!(home["entries"][3]["special"], "backups");

    call(&c.app, "DELETE", "/api/v0/parts/lidar", None).await;
    put_bytes(&c.app, "home:/Documents/notes.txt", b"v2").await;
    let (s, _) = call(&c.app, "POST", "/api/v0/backups/restore", Some(json!({"path": path}))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, parts) = call(&c.app, "GET", "/api/v0/parts", None).await;
    assert_eq!(parts[0]["name"], "lidar");
    assert_eq!(get_bytes(&c.app, "home:/Documents/notes.txt").await.1, b"v1");

    let (s, err) =
        call(&c.app, "POST", "/api/v0/backups/restore", Some(json!({"path": "home:/Documents/notes.txt"})))
            .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "backup-invalid");
    assert!(err["error"]["hint"].as_str().is_some_and(|h| !h.is_empty()));
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_can_be_reset_to_their_defaults() {
    let c = core();
    let (_, mut settings) = call(&c.app, "GET", "/api/v0/settings", None).await;
    assert_eq!(
        settings["accessibility"],
        json!({"reduceMotion": "system", "higherContrast": "system", "textSize": "100"})
    );
    assert_eq!(settings["units"], json!({"length": "metric"}));
    assert_eq!(settings["time"], json!({"timeZone": "", "clock": "24h"}));
    settings["units"]["length"] = json!("imperial");
    settings["time"]["timeZone"] = json!("Nowhere/Special");
    let (s, err) = call(&c.app, "PUT", "/api/v0/settings", Some(settings.clone())).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "invalid-time-zone");
    settings["time"]["timeZone"] = json!("Europe/Copenhagen");
    let (s, _) = call(&c.app, "PUT", "/api/v0/settings", Some(settings)).await;
    assert_eq!(s, StatusCode::OK);

    let (s, reset) = call(&c.app, "POST", "/api/v0/settings/reset", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(reset["units"]["length"], "metric");
    assert_eq!(reset["time"]["timeZone"], "");
    let (_, now) = call(&c.app, "GET", "/api/v0/settings", None).await;
    assert_eq!(now, reset);
}

#[tokio::test(flavor = "multi_thread")]
async fn big_files_can_be_downloaded_but_not_opened_inline() {
    let c = core();
    // A backup with recordings is easily bigger than the 16 MB that Notes may load.
    let size = 17 * 1024 * 1024 + 3;
    let real = c._dir.path().join("files").join("Documents").join("big.bin");
    std::fs::write(&real, vec![7u8; size]).unwrap();

    let (s, _) = get_bytes(&c.app, "home:/Documents/big.bin").await;
    assert_ne!(s, StatusCode::OK, "inline reads stay capped");

    let (s, body) = get_bytes(&c.app, "home:/Documents/big.bin&download=1").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body.len(), size);
    assert!(body.iter().all(|&b| b == 7));
}
