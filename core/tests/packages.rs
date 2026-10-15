//! Connector packages through the API (docs/packages.md §5, docs/api.md "Packages"): preview,
//! install while the core runs, parts on the connector restarting with a new version, and
//! uninstall refused while a part uses it. Declarative packages here; the wasm path is proven
//! end to end, from the template, in tools/faber-cli/tests/first_connector.rs.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_connector::capture::Chunk;
use faber_core::api;
use faber_core::clock::MonotonicClock;
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::packages;
use faber_core::ports::{ReplayLibrary, SystemOpener};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

struct Nothing;
impl faber_core::ports::SerialDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::ports::Discovered> {
        Vec::new()
    }
}
impl faber_core::ports::UsbProblemDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::ports::FailedUsb> {
        Vec::new()
    }
}
impl faber_core::disks::DiskDiscovery for Nothing {
    fn list(&self) -> Vec<faber_core::disks::Disk> {
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
    let library = ReplayLibrary::default();
    let lines: Vec<Chunk> =
        (0..50).map(|i| Chunk { t_us: i * 100_000, bytes: b"D:150\n".to_vec() }).collect();
    library.insert("bench", lines, None);
    let clock = Arc::new(MonotonicClock::new());
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(Nothing),
        usb_problems: Arc::new(Nothing),
        disks: Arc::new(Nothing),
        opener: Arc::new(SystemOpener::new(library.clone())),
        replays: library,
        connectors: faber_core::connectors::built_in(),
        timing: faber_core::runtime::Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    machine.refresh_ports();
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, dir }
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Body>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(path);
    let req = match body {
        Some(b) => req.header("content-type", "application/octet-stream").body(b).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
}

async fn json_call(app: &axum::Router, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

const SENSOR: &str = "faber-manifest = 1\n[package]\nid = \"my-sensor\"\ntype = \"connector\"\n\
    kind = \"declarative\"\nversion = \"0.1.0\"\nname = \"My sensor\"\npublisher = \"Ada\"\n\
    [ports]\nserial = { baud = 9600 }\n[[provides]]\ncapability = \"distance@1\"\n\
    line = { pattern = \"D:{meters:f32}\", scale = 0.01 }\n";

fn package(toml: &str) -> Vec<u8> {
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", toml.as_bytes()),
        ("README.md", b"# My sensor\n\nPrints D:<cm>.".as_slice()),
        ("LICENSE", b"MIT".as_slice()),
        ("captures/bench.fbcap", b"FABERCAP1\n".as_slice()),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d.to_vec()))
    .collect();
    packages::pack(&files).unwrap()
}

/// Polls a signal until `ok` accepts its reading (up to 5 s).
async fn reading_where(app: &axum::Router, address: &str, ok: impl Fn(&Value) -> bool) -> Value {
    let start = Instant::now();
    loop {
        let (status, v) = call(app, "GET", &format!("/api/v0/signals/{address}"), None).await;
        if status == StatusCode::OK && !v["reading"].is_null() && ok(&v["reading"]) {
            return v["reading"].clone();
        }
        assert!(start.elapsed() < Duration::from_secs(5), "{address}: {status} {v}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_preview_says_what_it_would_install_and_installs_nothing() {
    let c = core();
    let (s, p) =
        call(&c.app, "POST", "/api/v0/packages?mode=preview", Some(Body::from(package(SENSOR)))).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["connector"]["id"], "my-sensor");
    assert_eq!(p["connector"]["kind"], "declarative");
    assert_eq!(p["connector"]["port"], json!({"kind": "serial", "baud": 9600}));
    assert_eq!(p["publisher"], "Ada");
    assert_eq!(p["readme"], "# My sensor\n\nPrints D:<cm>.");
    assert_eq!(p["captures"], json!(["bench.fbcap"]));
    assert_eq!(p["checked"], false);
    assert!(p.get("replaces").is_none());
    assert_eq!(call(&c.app, "GET", "/api/v0/packages", None).await.1, json!([]));
    assert!(!c.dir.path().join("connectors/my-sensor").exists());
    assert!(c.machine.connectors().iter().all(|m| m.id != "my-sensor"));
}

#[tokio::test(flavor = "multi_thread")]
async fn installing_offers_the_connector_at_once_and_an_update_restarts_its_parts() {
    let c = core();
    let (s, info) = call(
        &c.app,
        "POST",
        "/api/v0/packages?name=my-sensor-0.1.0.faberpkg",
        Some(Body::from(package(SENSOR))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{info}");
    assert_eq!(info["id"], "my-sensor");
    assert_eq!(info["source"], "my-sensor-0.1.0.faberpkg");
    assert_eq!(info["checked"], false);
    assert_eq!(info["usedBy"], json!([]));
    assert!(info["installedAt"].as_str().unwrap().ends_with('Z'), "{info}");
    let listed = std::fs::read_to_string(c.dir.path().join("config/packages.toml")).unwrap();
    assert!(listed.contains("[packages.my-sensor]") && listed.contains("checked = false"), "{listed}");

    let (_, connectors) = call(&c.app, "GET", "/api/v0/connectors", None).await;
    assert!(connectors.as_array().unwrap().iter().any(|c| c["id"] == "my-sensor"), "no restart needed");

    let part = json!({"name": "front", "connector": "my-sensor", "port": "replay:bench"});
    let (s, body) = json_call(&c.app, "POST", "/api/v0/parts", part).await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    reading_where(&c.app, "front/distance", |r| r["reading"]["meters"] == 1.5).await;

    // Version 0.2.0 reads millimetres-as-centimetres differently: the part must pick it up.
    let v2 = SENSOR.replace("0.1.0", "0.2.0").replace("scale = 0.01", "scale = 0.001");
    let (_, preview) =
        call(&c.app, "POST", "/api/v0/packages?mode=preview", Some(Body::from(package(&v2)))).await;
    assert_eq!(preview["replaces"], "0.1.0");
    let (s, info) = call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(&v2)))).await;
    assert_eq!(s, StatusCode::CREATED, "{info}");
    assert_eq!((info["version"].as_str(), info["usedBy"].clone()), (Some("0.2.0"), json!(["front"])));
    assert_eq!(info["source"], "upload.faberpkg");
    reading_where(&c.app, "front/distance", |r| r["reading"]["meters"] == 0.15).await;
    let (_, parts) = call(&c.app, "GET", "/api/v0/parts", None).await;
    assert_eq!(parts[0]["connector"], "my-sensor");
}

#[tokio::test(flavor = "multi_thread")]
async fn uninstall_is_refused_while_a_part_uses_it_and_names_the_part() {
    let c = core();
    call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(SENSOR)))).await;
    let part = json!({"name": "front", "connector": "my-sensor", "port": "replay:bench"});
    json_call(&c.app, "POST", "/api/v0/parts", part).await;

    let (s, e) = call(&c.app, "DELETE", "/api/v0/packages/my-sensor", None).await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    assert_eq!(e["error"]["code"], "package-in-use");
    assert!(e["error"]["message"].as_str().unwrap().contains("the part \"front\""), "{e}");
    assert!(e["error"]["hint"].as_str().unwrap().contains("Parts window"), "{e}");

    let (s, _) = call(&c.app, "DELETE", "/api/v0/parts/front", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&c.app, "DELETE", "/api/v0/packages/my-sensor", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(call(&c.app, "GET", "/api/v0/packages", None).await.1, json!([]));
    assert!(c.machine.connectors().iter().all(|m| m.id != "my-sensor"));
    assert!(!c.dir.path().join("connectors/my-sensor").exists());

    let (s, e) = call(&c.app, "DELETE", "/api/v0/packages/my-sensor", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not-installed")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_package_that_would_not_load_is_refused_with_a_hint_and_changes_nothing() {
    let c = core();
    let cases = [
        (b"not a zip".to_vec(), StatusCode::UNPROCESSABLE_ENTITY, "package-invalid", "isn't a zip"),
        (
            package(&SENSOR.replace("{meters:f32}", "{meters}")),
            StatusCode::UNPROCESSABLE_ENTITY,
            "package-not-loadable",
            "faber.toml line 13",
        ),
        (
            package(&SENSOR.replace("my-sensor", "ldrobot-ld19")),
            StatusCode::CONFLICT,
            "package-id-taken",
            "built into Faber",
        ),
    ];
    for (bytes, status, code, words) in cases {
        let (s, e) = call(&c.app, "POST", "/api/v0/packages", Some(Body::from(bytes))).await;
        assert_eq!((s, e["error"]["code"].as_str()), (status, Some(code)), "{e}");
        assert!(e["error"]["message"].as_str().unwrap().contains(words), "{words}: {e}");
        assert!(!e["error"]["hint"].as_str().unwrap().is_empty());
    }
    let (s, e) = call(&c.app, "POST", "/api/v0/packages?mode=later", Some(Body::from(package(SENSOR)))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let too_big = vec![0u8; packages::MAX_BYTES as usize + 2];
    let (s, e) = call(&c.app, "POST", "/api/v0/packages", Some(Body::from(too_big))).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("package-too-big")));
    assert_eq!(call(&c.app, "GET", "/api/v0/packages", None).await.1, json!([]));
    assert!(
        !c.dir.path().join("connectors").exists()
            || std::fs::read_dir(c.dir.path().join("connectors")).unwrap().next().is_none()
    );
}

/// Regression: a package that fails to load at install time is still in staging, so its problem
/// names the package file and the file inside it, never `connectors/<id>/…`.
#[tokio::test(flavor = "multi_thread")]
async fn a_load_problem_names_the_package_file_and_the_file_inside_it() {
    let c = core();
    let broken = package(&SENSOR.replace("{meters:f32}", "{meters}"));
    for mode in ["", "&mode=preview"] {
        let path = format!("/api/v0/packages?name=my-sensor-0.1.0.faberpkg{mode}");
        let (s, e) = call(&c.app, "POST", &path, Some(Body::from(broken.clone()))).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("package-not-loadable"))
        );
        let message = e["error"]["message"].as_str().unwrap();
        assert!(message.starts_with("my-sensor-0.1.0.faberpkg: faber.toml line 13: "), "{mode}: {message}");
        assert!(!message.contains("connectors/"), "{mode}: {message}");
    }
    // A wasm package whose faber.toml only the full reader refuses (the header reads fine).
    let wasm = SENSOR.replace("declarative", "wasm").replace("type = \"connector\"", "type = \"app\"");
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", wasm.as_bytes()),
        ("README.md", b"# My sensor".as_slice()),
        ("LICENSE", b"MIT".as_slice()),
        ("connector.wasm", b"\0asm".as_slice()),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d.to_vec()))
    .collect();
    let bytes = packages::pack(&files).unwrap();
    let (s, e) = call(&c.app, "POST", "/api/v0/packages?name=w.faberpkg", Some(Body::from(bytes))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{e}");
    let message = e["error"]["message"].as_str().unwrap();
    assert!(message.starts_with("w.faberpkg: faber.toml"), "{message}");
}

mod ws {
    use super::*;
    use faber_core::api::dto;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

    async fn connect(app: axum::Router) -> Ws {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (ws, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=msgpack")).await.unwrap();
        ws
    }

    /// The next `packages` message, as (id, version) pairs, within 5 s.
    async fn next_packages(ws: &mut Ws) -> Vec<(String, String)> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let Message::Binary(b) = ws.next().await.expect("socket open").unwrap() else { continue };
                if let dto::ServerMessage::Packages { id: 1, packages } = rmp_serde::from_slice(&b).unwrap() {
                    return packages.into_iter().map(|p| (p.id, p.version)).collect();
                }
            }
        })
        .await
        .expect("a packages message within 5 s")
    }

    /// Every other browser tab learns about installs: the list on subscribe, then after every
    /// install (including an update) and uninstall.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_packages_topic_sends_the_list_now_and_after_every_change() {
        let c = core();
        let mut ws = connect(c.app.clone()).await;
        let subscribe = dto::ClientMessage::Subscribe { id: 1, topic: dto::Topic::Packages };
        ws.send(Message::Binary(rmp_serde::to_vec_named(&subscribe).unwrap().into())).await.unwrap();
        assert_eq!(next_packages(&mut ws).await, []);

        call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(SENSOR)))).await;
        let sensor = |v: &str| vec![("my-sensor".to_string(), v.to_string())];
        assert_eq!(next_packages(&mut ws).await, sensor("0.1.0"));

        let v2 = SENSOR.replace("0.1.0", "0.2.0");
        call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(&v2)))).await;
        assert_eq!(next_packages(&mut ws).await, sensor("0.2.0"));

        // A preview and a refused package change nothing, so nothing is announced for them.
        call(&c.app, "POST", "/api/v0/packages?mode=preview", Some(Body::from(package(SENSOR)))).await;
        call(&c.app, "DELETE", "/api/v0/packages/my-sensor", None).await;
        assert_eq!(next_packages(&mut ws).await, []);
    }
}

mod backups {
    use super::*;
    use std::io::{Read, Write};

    async fn back_up(c: &TestCore) -> String {
        let (s, b) = json_call(&c.app, "POST", "/api/v0/backups", json!({"includeRecordings": false})).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["path"].as_str().unwrap().to_string()
    }

    async fn restore(c: &TestCore, path: &str) -> (StatusCode, Value) {
        json_call(&c.app, "POST", "/api/v0/backups/restore", json!({ "path": path })).await
    }

    fn on_disk(c: &TestCore, path: &str) -> std::path::PathBuf {
        c.dir.path().join("files").join(path.strip_prefix("home:/").unwrap())
    }

    /// One way to tamper with a backup file.
    type Tamper<'a> = dyn Fn(&std::path::Path) + 'a;

    /// Rewrites a backup zip, changing or adding entries.
    fn tamper(
        file: &std::path::Path,
        change: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>,
        add: &[(&str, &[u8])],
    ) {
        let bytes = std::fs::read(file).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut out = zip::ZipWriter::new(std::fs::File::create(file).unwrap());
        for i in 0..zip.len() {
            let mut e = zip.by_index(i).unwrap();
            let name = e.name().unwrap().into_owned();
            if e.is_dir() {
                out.add_directory(name, zip::write::SimpleFileOptions::default()).unwrap();
                continue;
            }
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            if let Some(data) = change(&name, data) {
                out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                out.write_all(&data).unwrap();
            }
        }
        for (name, data) in add {
            out.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            out.write_all(data).unwrap();
        }
        out.finish().unwrap();
    }

    async fn package_ids(c: &TestCore) -> Vec<(String, String)> {
        let (_, list) = call(&c.app, "GET", "/api/v0/packages", None).await;
        list.as_array()
            .unwrap()
            .iter()
            .map(|p| (p["id"].as_str().unwrap().to_string(), p["version"].as_str().unwrap().to_string()))
            .collect()
    }

    /// A restored connector at the installed version is left alone; another version replaces the
    /// installed one exactly like an install, and the parts on it restart with it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_restored_connector_is_skipped_at_the_same_version_and_replaces_another() {
        let c = core();
        call(
            &c.app,
            "POST",
            "/api/v0/packages?name=my-sensor-0.1.0.faberpkg",
            Some(Body::from(package(SENSOR))),
        )
        .await;
        let part = json!({"name": "front", "connector": "my-sensor", "port": "replay:bench"});
        json_call(&c.app, "POST", "/api/v0/parts", part).await;
        let path = back_up(&c).await;

        let (_, before) = call(&c.app, "GET", "/api/v0/packages", None).await;
        assert_eq!(restore(&c, &path).await.0, StatusCode::NO_CONTENT);
        let (_, after) = call(&c.app, "GET", "/api/v0/packages", None).await;
        assert_eq!(after[0]["installedAt"], before[0]["installedAt"], "the same version is skipped");

        let v2 = SENSOR.replace("0.1.0", "0.2.0").replace("scale = 0.01", "scale = 0.001");
        call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(&v2)))).await;
        reading_where(&c.app, "front/distance", |r| r["reading"]["meters"] == 0.15).await;
        assert_eq!(restore(&c, &path).await.0, StatusCode::NO_CONTENT);
        assert_eq!(package_ids(&c).await, [("my-sensor".to_string(), "0.1.0".to_string())]);
        let (_, after) = call(&c.app, "GET", "/api/v0/packages", None).await;
        assert_eq!(after[0]["source"], "my-sensor-0.1.0.faberpkg", "where it first came from");
        reading_where(&c.app, "front/distance", |r| r["reading"]["meters"] == 1.5).await;
    }

    /// Connector code in a backup is never copied in blind: it goes through the package checks
    /// and the staging load first, and a connector that fails fails the whole restore.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_tampered_connector_in_a_backup_is_refused_and_nothing_changes() {
        let c = core();
        call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(SENSOR)))).await;
        let path = back_up(&c).await;
        call(&c.app, "DELETE", "/api/v0/packages/my-sensor", None).await;
        let file = on_disk(&c, &path);
        let original = std::fs::read(&file).unwrap();
        let parts_before = std::fs::read_to_string(c.dir.path().join("config/parts.toml")).unwrap();

        let broken_rule = |name: &str, data: Vec<u8>| {
            Some(if name == "connectors/my-sensor/faber.toml" {
                String::from_utf8(data).unwrap().replace("{meters:f32}", "{meters}").into_bytes()
            } else {
                data
            })
        };
        let cases: [(&Tamper<'_>, &str); 3] = [
            (&|f| tamper(f, broken_rule, &[]), "connectors/my-sensor/faber.toml line 13"),
            (&|f| tamper(f, |_, d| Some(d), &[("connectors/my-sensor/run.sh", b"rm -rf /")]), "run.sh"),
            (&|f| tamper(f, |n, d| (n != "config/packages.toml").then_some(d), &[]), "packages.toml"),
        ];
        for (make, words) in cases {
            std::fs::write(&file, &original).unwrap();
            make(&file);
            std::fs::write(c.dir.path().join("files/Documents/marker.txt"), "keep").unwrap();
            let (s, e) = restore(&c, &path).await;
            assert_eq!(
                (s, e["error"]["code"].as_str()),
                (StatusCode::UNPROCESSABLE_ENTITY, Some("backup-invalid")),
                "{e}"
            );
            let message = e["error"]["message"].as_str().unwrap();
            assert!(message.contains("\"my-sensor\"") && message.contains(words), "{words}: {message}");
            assert!(!e["error"]["hint"].as_str().unwrap().is_empty());
            assert_eq!(package_ids(&c).await, [], "{words}: nothing installed");
            assert!(!c.dir.path().join("connectors/my-sensor").exists(), "{words}");
            assert!(!c.dir.path().join("packages/staging/my-sensor").exists(), "{words}: staging cleaned");
            assert_eq!(
                std::fs::read_to_string(c.dir.path().join("config/parts.toml")).unwrap(),
                parts_before
            );
        }
    }

    /// A backup from before connectors were backed up still restores, and leaves the installed
    /// packages as they are.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_old_backup_without_connectors_still_restores() {
        let c = core();
        call(&c.app, "POST", "/api/v0/packages", Some(Body::from(package(SENSOR)))).await;
        let file = c.dir.path().join("files/Backups/faber-backup-2026-10-01-1200.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("config/parts.toml", o).unwrap();
        w.write_all(b"[parts.front]\nconnector = \"my-sensor\"\nport = \"replay:bench\"\n").unwrap();
        w.start_file("files/Documents/old.txt", o).unwrap();
        w.write_all(b"from before").unwrap();
        w.finish().unwrap();

        let (s, e) = restore(&c, "home:/Backups/faber-backup-2026-10-01-1200.zip").await;
        assert_eq!(s, StatusCode::NO_CONTENT, "{e}");
        assert_eq!(package_ids(&c).await, [("my-sensor".to_string(), "0.1.0".to_string())]);
        assert_eq!(
            std::fs::read_to_string(c.dir.path().join("files/Documents/old.txt")).unwrap(),
            "from before"
        );
        reading_where(&c.app, "front/distance", |r| r["reading"]["meters"] == 1.5).await;
    }
}
