//! Declarative connectors end to end: a `faber.toml` in `<data>/connectors/<id>/` is loaded when
//! the core starts, listed by the API, and runs a part on a replay port like any connector.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_connector::capture::Chunk;
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{ReplayLibrary, SystemOpener};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

struct NoPorts;
impl faber_core::ports::SerialDiscovery for NoPorts {
    fn list(&self) -> Vec<faber_core::ports::Discovered> {
        Vec::new()
    }
}
struct NoUsbProblems;
impl faber_core::ports::UsbProblemDiscovery for NoUsbProblems {
    fn list(&self) -> Vec<faber_core::ports::FailedUsb> {
        Vec::new()
    }
}
struct NoDisks;
impl faber_core::disks::DiskDiscovery for NoDisks {
    fn list(&self) -> Vec<faber_core::disks::Disk> {
        Vec::new()
    }
}

pub struct TestCore {
    pub app: axum::Router,
    machine: Arc<Machine>,
    _dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

/// A core whose data folder has `connectors` installed (`(id, faber.toml text)`) and whose
/// replay ports are `replays` (`(name, chunks)`).
pub fn core(connectors: &[(&str, &str)], replays: Vec<(&str, Vec<Chunk>)>) -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    for (id, text) in connectors {
        let folder = dir.path().join("connectors").join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("faber.toml"), text).unwrap();
    }
    let library = ReplayLibrary::default();
    for (name, chunks) in replays {
        library.insert(name, chunks, None);
    }
    let clock = Arc::new(MonotonicClock::new());
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(NoPorts),
        usb_problems: Arc::new(NoUsbProblems),
        disks: Arc::new(NoDisks),
        opener: Arc::new(SystemOpener::new(library.clone())),
        replays: library,
        connectors: faber_core::all_connectors(dir.path()),
        timing: faber_core::runtime::Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    machine.refresh_ports();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, _dir: dir }
}

pub async fn call(
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

/// Polls `GET /api/v0/signals/<address>` (up to 5 s) until it has a reading.
pub async fn first_reading(app: &axum::Router, address: &str) -> serde_json::Value {
    let start = std::time::Instant::now();
    loop {
        let (status, v) = call(app, "GET", &format!("/api/v0/signals/{address}"), None).await;
        if status == StatusCode::OK && !v["reading"].is_null() {
            return v["reading"].clone();
        }
        assert!(start.elapsed() < Duration::from_secs(5), "{address} never published: {status} {v}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// `lines` as a capture, one line per chunk, 100 ms apart.
pub fn capture_of(lines: &[String]) -> Vec<Chunk> {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| Chunk { t_us: i as u64 * 100_000, bytes: l.as_bytes().to_vec() })
        .collect()
}

const SENSOR: &str = r#"faber-manifest = 1
[package]
id = "my-sensor"
type = "connector"
kind = "declarative"
version = "0.1.0"
name = "My sensor"

[ports]
serial = { baud = 9600 }

[[provides]]
capability = "distance@1"
line = { pattern = "D:{meters:f32}", scale = 0.01 }
"#;

#[tokio::test(flavor = "multi_thread")]
async fn an_installed_faber_toml_is_a_connector_like_the_built_in_ones() {
    let lines = vec!["D:150\n".to_string(); 5];
    let c = core(&[("my-sensor", SENSOR)], vec![("bench", capture_of(&lines))]);

    let (_, connectors) = call(&c.app, "GET", "/api/v0/connectors", None).await;
    let all = connectors.as_array().unwrap();
    let mine = all.iter().find(|c| c["id"] == "my-sensor").expect("listed");
    assert_eq!(mine["kind"], "declarative");
    assert_eq!(mine["port"], json!({"kind": "serial", "baud": 9600}));
    assert_eq!(mine["provides"][0]["capability"], "distance@1");
    let ld19 = all.iter().find(|c| c["id"] == "ldrobot-ld19").unwrap();
    assert_eq!(ld19["kind"], "built-in");

    let part = json!({"name": "front", "connector": "my-sensor", "port": "replay:bench"});
    let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(part)).await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let reading = first_reading(&c.app, "front/distance").await;
    assert_eq!(reading["capability"], "distance@1");
    assert_eq!(reading["reading"]["meters"], 1.5);
    assert_eq!(reading["reading"]["status"], "ok");
}

#[test]
fn a_broken_faber_toml_is_skipped_and_the_core_still_starts() {
    let c = core(&[("my-sensor", &SENSOR.replace("{meters:f32}", "{meters}"))], vec![]);
    let ids: Vec<String> = c.machine.connectors().into_iter().map(|m| m.id).collect();
    assert!(!ids.contains(&"my-sensor".to_string()) && ids.contains(&"ldrobot-ld19".to_string()), "{ids:?}");
}

#[test]
fn an_installed_connector_cannot_replace_a_built_in_one() {
    let text = SENSOR.replace("my-sensor", "ldrobot-ld19");
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("connectors").join("ldrobot-ld19");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("faber.toml"), text).unwrap();
    let defs = faber_core::all_connectors(dir.path());
    let ld19: Vec<_> = defs.iter().filter(|d| d.manifest.id == "ldrobot-ld19").collect();
    assert_eq!(ld19.len(), 1);
    assert_eq!(ld19[0].kind, faber_connector::ConnectorKind::BuiltIn);
}
