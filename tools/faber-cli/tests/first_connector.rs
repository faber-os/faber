//! The proof of Phase 3 (docs/roadmap.md): "a third party could write a connector from the
//! template, and the core runs it sandboxed". Exactly what docs/your-first-connector.md tells a
//! contributor to do, in order:
//!
//! 1. `faber new connector acme-ranger` makes a project from the template, outside Faber's tree;
//! 2. its own `cargo test` passes against the recorded capture, before a line is changed;
//! 3. `faber check` builds it for wasm32-unknown-unknown, loads it in the sandbox and replays the
//!    capture; `faber pack` writes `acme-ranger-0.1.0.faberpkg`;
//! 4. the package installs through the API on a running core, with no restart;
//! 5. a part on a replay port of the capture gives readings;
//! 6. uninstalling is refused while that part exists, and works once it is removed.
//!
//! Needs the wasm32-unknown-unknown target: without it the test says so and passes, unless
//! FABER_REQUIRE_WASM is set (CI), which makes a missing target a failure.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_cli::check::{self, Options};
use faber_cli::pack;
use faber_cli::template::{self, Kind, SAMPLE};
use faber_connector::capture;
use faber_core::clock::MonotonicClock;
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::ports::{ReplayLibrary, SystemOpener};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
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

/// Builds go into the workspace's target folder (shared with core/tests/wasm.rs), so the SDK and
/// wit-bindgen compile once. A folder of its own, not the one this test runs from: that one is
/// locked by the cargo running the tests.
fn guest_target() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    workspace
        .join(std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| "target".into()))
        .join("wasm-guests")
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Body, json: bool) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if json {
        req = req.header("content-type", "application/json");
    }
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_connector_from_the_template_installs_and_runs_sandboxed() {
    if !check::have_target() {
        assert!(
            std::env::var_os("FABER_REQUIRE_WASM").is_none(),
            "FABER_REQUIRE_WASM is set, but the wasm32-unknown-unknown target is missing"
        );
        eprintln!("skipping: run `rustup target add wasm32-unknown-unknown` to run this test");
        return;
    }
    // 1. A new project, somewhere that isn't the Faber checkout.
    let work = tempfile::tempdir().unwrap();
    let project = template::new_connector("acme-ranger", Kind::Wasm, work.path()).unwrap();

    // 2. Its own tests pass on day one.
    let target = guest_target();
    let tested = Command::new(env!("CARGO"))
        .current_dir(&project)
        .args(["test", "--quiet", "--target-dir"])
        .arg(&target)
        .status()
        .unwrap();
    assert!(tested.success(), "cargo test in the new project failed");

    // 3. faber check and faber pack, as library calls.
    let opts = Options { target_dir: Some(target) };
    let mut log = Vec::new();
    let report = check::check(&project, &opts, &mut log).unwrap_or_else(|e| panic!("{e}"));
    assert!(report.passed(), "{:?}", report.lines());
    assert_eq!(report.captures[0].readings.get("distance"), Some(&5));
    assert_eq!(report.captures[0].last.as_deref(), Some("0.400 m, ok"));
    let file = pack::pack(&project, None, &opts, &mut log).unwrap_or_else(|e| panic!("{e}"));
    assert!(file.ends_with("acme-ranger-0.1.0.faberpkg"), "{}", file.display());
    let log = String::from_utf8(log).unwrap();
    assert!(log.contains("loads in the sandbox"), "{log}");

    // 4. Install it on a running core.
    let data = tempfile::tempdir().unwrap();
    let replays = ReplayLibrary::default();
    let chunks = capture::decode(&std::fs::read(project.join(SAMPLE)).unwrap()).unwrap();
    replays.insert("acme-sample", chunks, None);
    let clock = Arc::new(MonotonicClock::new());
    let machine = Machine::start(MachineOptions {
        data_dir: data.path().to_path_buf(),
        clock: clock.clone(),
        discovery: Arc::new(Nothing),
        usb_problems: Arc::new(Nothing),
        disks: Arc::new(Nothing),
        opener: Arc::new(SystemOpener::new(replays.clone())),
        hardware: Arc::new(faber_core::ports::NoHardware),
        replays,
        connectors: faber_core::connectors::built_in(),
        timing: faber_core::runtime::Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    machine.refresh_ports();
    // Signed in as a program is: the first admin's API token on every request (docs/auth.md).
    let (admin, _) = machine.create_first_admin("tester", "test password").unwrap();
    let (_, token) = machine.create_token("tests", faber_core::users::UserRole::Admin, &admin).unwrap();
    let bearer = axum::http::HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
    let app =
        faber_core::api::router(faber_core::api::AppState::new(Arc::clone(&machine), LogBook::new(clock)))
            .layer(axum::middleware::map_request(move |mut req: axum::extract::Request| {
                let bearer = bearer.clone();
                async move {
                    req.headers_mut().insert(axum::http::header::AUTHORIZATION, bearer);
                    req
                }
            }));
    let bytes = std::fs::read(&file).unwrap();
    let (s, preview) =
        call(&app, "POST", "/api/v0/packages?mode=preview", Body::from(bytes.clone()), false).await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    assert_eq!(preview["connector"]["kind"], "wasm");
    assert_eq!(preview["publisher"], "Your name");
    let (s, info) =
        call(&app, "POST", "/api/v0/packages?name=acme-ranger-0.1.0.faberpkg", Body::from(bytes), false)
            .await;
    assert_eq!(s, StatusCode::CREATED, "{info}");
    let (_, connectors) = call(&app, "GET", "/api/v0/connectors", Body::empty(), false).await;
    let mine = connectors.as_array().unwrap().iter().find(|c| c["id"] == "acme-ranger").expect("offered");
    assert_eq!((mine["kind"].as_str(), mine["name"].as_str()), (Some("wasm"), Some("Acme ranger")));

    // 5. A part on the recorded capture gives readings, from inside the sandbox.
    let part = json!({"name": "ranger", "connector": "acme-ranger", "port": "replay:acme-sample"});
    let (s, body) = call(&app, "POST", "/api/v0/parts", Body::from(part.to_string()), true).await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let started = Instant::now();
    let reading = loop {
        let (s, v) = call(&app, "GET", "/api/v0/signals/ranger/distance", Body::empty(), false).await;
        if s == StatusCode::OK && !v["reading"].is_null() {
            break v["reading"].clone();
        }
        assert!(started.elapsed() < Duration::from_secs(10), "no reading: {s} {v}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(reading["capability"], "distance@1");
    assert_eq!(reading["reading"]["status"], "ok");

    // 6. Uninstall: refused while the part exists, then allowed.
    let (s, e) = call(&app, "DELETE", "/api/v0/packages/acme-ranger", Body::empty(), false).await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    assert!(e["error"]["message"].as_str().unwrap().contains("\"ranger\""), "{e}");
    let (s, _) = call(&app, "DELETE", "/api/v0/parts/ranger", Body::empty(), false).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = call(&app, "DELETE", "/api/v0/packages/acme-ranger", Body::empty(), false).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{e}");
    let (_, connectors) = call(&app, "GET", "/api/v0/connectors", Body::empty(), false).await;
    assert!(connectors.as_array().unwrap().iter().all(|c| c["id"] != "acme-ranger"));
    machine.shutdown();
}
