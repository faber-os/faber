//! Wasm connectors (docs/packages.md §2–4). The proof of Phase 3: the LD19 built as a
//! WebAssembly component gives exactly the readings of the built-in LD19 on a real capture. Plus
//! the sandbox: time and memory limits, traps, undeclared signals, WASI imports, and restarts.
//!
//! The components are built here from the workspace's own crates (`cargo rustc` for
//! `wasm32-unknown-unknown`, then wrapped as a component), so nothing binary is checked in. On a
//! machine without that target the tests say so and pass; CI sets `FABER_REQUIRE_WASM=1`, which
//! makes a missing target a failure.

use faber_capabilities::{Command, motor};
use faber_connector::testing::RecordingContext;
use faber_connector::{Applied, CommandError, Connector, ConnectorKind, Health, Manifest, Problem, Settings};
use faber_core::wasm::{self, Limits};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TARGET: &str = "wasm32-unknown-unknown";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Whether the wasm target is installed for the toolchain running these tests.
fn have_target() -> bool {
    let rustc = Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    let rustc = if rustc.exists() { rustc } else { PathBuf::from("rustc") };
    let Ok(out) = Process::new(rustc).args(["--print", "target-libdir", "--target", TARGET]).output() else {
        return false;
    };
    let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    out.status.success() && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

/// The component built from a workspace crate, or `None` (with a message) when this machine
/// can't build wasm. Built once per test run.
fn component(package: &str) -> Option<Vec<u8>> {
    static BUILT: Mutex<Option<HashMap<String, Vec<u8>>>> = Mutex::new(None);
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    let built = built.get_or_insert_with(HashMap::new);
    if let Some(c) = built.get(package) {
        return Some(c.clone());
    }
    if !have_target() {
        assert!(
            std::env::var_os("FABER_REQUIRE_WASM").is_none(),
            "FABER_REQUIRE_WASM is set, but the {TARGET} target is missing"
        );
        eprintln!(
            "skipping the wasm connector tests: the {TARGET} target is not installed. \
             Run `rustup target add {TARGET}` to run them."
        );
        return None;
    }
    // Cargo reads a relative CARGO_TARGET_DIR from the workspace, but this test runs in core/:
    // resolve it the way Cargo does, so the build and the read agree.
    let target_dir = workspace()
        .join(std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| "target".into()))
        .join("wasm-guests");
    let status = Process::new(env!("CARGO"))
        .current_dir(workspace())
        .env("CARGO_TARGET_DIR", &target_dir)
        .args(["rustc", "-p", package, "--lib", "--release", "--target", TARGET, "--crate-type", "cdylib"])
        .status()
        .expect("run cargo");
    assert!(status.success(), "building {package} for {TARGET} failed");
    let file = target_dir.join(TARGET).join("release").join(format!("{}.wasm", package.replace('-', "_")));
    let module = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let component = wit_component::ComponentEncoder::default()
        .module(&module)
        .expect("a module with component type information")
        .validate(true)
        .encode()
        .expect("wrap the module as a component");
    built.insert(package.into(), component.clone());
    Some(component)
}

fn capture() -> Vec<faber_connector::capture::Chunk> {
    let path = workspace().join("fixtures/captures/ld19-room-walkthrough.fbcap");
    faber_connector::capture::decode(&std::fs::read(path).unwrap()).unwrap()
}

/// Feeds a capture to a connector the way the runtime does: ticks every 100 ms between chunks.
fn replay(c: &mut dyn Connector, chunks: &[faber_connector::capture::Chunk]) -> RecordingContext {
    let mut ctx = RecordingContext::new();
    let mut next_tick = chunks[0].t_us;
    for chunk in chunks {
        while next_tick <= chunk.t_us {
            c.on_tick(&mut ctx, next_tick);
            next_tick += 100_000;
        }
        c.on_bytes(&mut ctx, chunk.t_us, &chunk.bytes);
    }
    ctx
}

#[test]
fn the_wasm_ld19_gives_exactly_the_readings_of_the_built_in_one() {
    let Some(wasm) = component("ldrobot-ld19") else { return };
    let chunks = capture();
    let started = Instant::now();
    let def = wasm::definition(ldrobot_ld19::manifest(), &wasm, Limits::default()).expect("loads");
    let compile_time = started.elapsed();
    assert_eq!(def.kind, ConnectorKind::Wasm);

    let started = Instant::now();
    let built_in = replay((ldrobot_ld19::definition().create)(&Settings::new()).unwrap().as_mut(), &chunks);
    let built_in_time = started.elapsed();
    let started = Instant::now();
    let sandboxed = replay((def.create)(&Settings::new()).unwrap().as_mut(), &chunks);
    let wasm_time = started.elapsed();

    assert!(built_in.published.len() > 100, "the capture has many revolutions");
    assert_eq!(sandboxed.published.len(), built_in.published.len(), "as many readings");
    for (i, (w, b)) in sandboxed.published.iter().zip(&built_in.published).enumerate() {
        assert_eq!(w, b, "reading {i} differs");
    }
    assert_eq!(sandboxed.health, built_in.health, "the same health, in the same order");

    let per_s = |d: Duration| built_in.published.len() as f64 / d.as_secs_f64();
    eprintln!(
        "{} readings: built-in {:.0}/s ({built_in_time:?}), wasm {:.0}/s ({wasm_time:?}); compiling the component took {compile_time:?}",
        built_in.published.len(),
        per_s(built_in_time),
        per_s(wasm_time)
    );
}

#[test]
fn the_wasm_ld19_refuses_settings_like_the_built_in_one() {
    let Some(wasm) = component("ldrobot-ld19") else { return };
    let def = wasm::definition(ldrobot_ld19::manifest(), &wasm, Limits::default()).unwrap();
    let mut s = Settings::new();
    s.insert("speed".into(), 10.into());
    let built_in = (ldrobot_ld19::definition().create)(&s).err().unwrap();
    let sandboxed = (def.create)(&s).err().unwrap();
    assert_eq!(sandboxed, built_in);
}

#[test]
fn the_ld19_component_imports_nothing_but_the_connector_world() {
    let Some(wasm) = component("ldrobot-ld19") else { return };
    let engine = wasmtime::Engine::default();
    let c = wasmtime::component::Component::new(&engine, &wasm).unwrap();
    let imports: Vec<String> = c.component_type().imports(&engine).map(|(n, _)| n.to_string()).collect();
    assert!(imports.contains(&"faber:connector/context@0.1.0".to_string()), "{imports:?}");
    assert!(imports.iter().all(|i| i.starts_with("faber:")), "no WASI: {imports:?}");
}

fn guest() -> Option<Box<dyn Connector>> {
    guest_with(Limits::default())
}

fn guest_with(limits: Limits) -> Option<Box<dyn Connector>> {
    let wasm = component("faber-test-guest")?;
    let def = wasm::definition(faber_test_guest::manifest(), &wasm, limits).expect("loads");
    Some((def.create)(&Settings::new()).expect("creates"))
}

/// Sends one byte, and returns the problem the call ended with, if it ended with one.
fn send(c: &mut dyn Connector, ctx: &mut RecordingContext, byte: u8) -> Option<Problem> {
    match catch_unwind(AssertUnwindSafe(|| c.on_bytes(ctx, 7, &[byte]))) {
        Ok(()) => None,
        Err(payload) => Some(*payload.downcast::<Problem>().expect("a sandboxed call ends with a Problem")),
    }
}

#[test]
fn a_well_behaved_call_publishes_and_reports_health() {
    let Some(mut c) = guest() else { return };
    let mut ctx = RecordingContext::new();
    assert_eq!(send(c.as_mut(), &mut ctx, faber_test_guest::byte::PUBLISH), None);
    assert_eq!(ctx.health, [Health::Running]);
    assert_eq!(ctx.published.len(), 1);
    assert_eq!(ctx.published[0].0, "distance");
}

#[test]
fn a_connector_that_never_returns_is_stopped_after_its_time() {
    let Some(mut c) = guest() else { return };
    let mut ctx = RecordingContext::new();
    let started = Instant::now();
    let p = send(c.as_mut(), &mut ctx, faber_test_guest::byte::LOOP_FOREVER).expect("stopped");
    let took = started.elapsed();
    assert_eq!(p.message, "Test guest took longer than 20 ms to handle data and was stopped.");
    assert!(p.hint.contains("restarts it automatically"), "{p:?}");
    assert!(took >= Duration::from_millis(20), "it had its 20 ms: {took:?}");
    assert!(took < Duration::from_millis(500), "and not much more: {took:?}");
    let again = send(c.as_mut(), &mut ctx, faber_test_guest::byte::PUBLISH);
    assert_eq!(again, Some(p), "a stopped instance stays stopped until the runtime makes a new one");
}

#[test]
fn a_connector_that_needs_too_much_memory_is_stopped() {
    let Some(mut c) = guest() else { return };
    let p =
        send(c.as_mut(), &mut RecordingContext::new(), faber_test_guest::byte::HOG_MEMORY).expect("stopped");
    assert_eq!(p.message, "Test guest needed more than 32 MB of memory and was stopped.");
}

#[test]
fn the_memory_limit_is_per_instance_and_configurable() {
    let Some(mut c) = guest_with(Limits { memory_bytes: 128 << 20, ..Limits::default() }) else { return };
    assert_eq!(send(c.as_mut(), &mut RecordingContext::new(), faber_test_guest::byte::HOG_MEMORY), None);
}

#[test]
fn a_panic_is_a_crash_that_says_what_panicked() {
    let Some(mut c) = guest() else { return };
    let p = send(c.as_mut(), &mut RecordingContext::new(), faber_test_guest::byte::PANIC).expect("stopped");
    assert_eq!(p.message, "The connector stopped because of a bug: it panicked (bad byte).");
}

#[test]
fn publishing_an_undeclared_signal_is_refused_and_stops_the_part() {
    let Some(mut c) = guest() else { return };
    let mut ctx = RecordingContext::new();
    let p = send(c.as_mut(), &mut ctx, faber_test_guest::byte::UNDECLARED).expect("stopped");
    assert!(ctx.published.is_empty(), "nothing reached the part: {:?}", ctx.published);
    assert!(p.message.contains("published to \"elsewhere\""), "{p:?}");
}

#[test]
fn commands_reach_the_component_and_come_back_applied() {
    let Some(mut c) = guest() else { return };
    let mut ctx = RecordingContext::new();
    let duty = |d| Command::Motor(motor::Command::SetDuty { duty: d });
    assert_eq!(c.command(&mut ctx, duty(0.5)), Ok(Applied { command: duty(0.25) }));
    assert_eq!(c.command(&mut ctx, Command::Motor(motor::Command::STOP)), Err(CommandError::Unsupported));
}

fn manifest(name: &str) -> Manifest {
    Manifest { name: name.into(), ..faber_test_guest::manifest() }
}

#[test]
fn a_component_that_asks_for_wasi_is_refused() {
    let wat = r#"(component
        (import "wasi:cli/environment@0.2.0" (instance
            (export "get-arguments" (func (result (list string))))))
    )"#;
    let p = wasm::definition(manifest("Nosy"), wat.as_bytes(), Limits::default()).expect_err("refused");
    assert_eq!(p.message, "Nosy asks for wasi:cli/environment@0.2.0, which Faber doesn't give connectors.");
}

#[test]
fn a_component_that_asks_for_an_app_command_is_refused() {
    let wat = r#"(component
        (import "faber:capabilities/motor@0.1.0" (instance (export "set-duty" (func))))
    )"#;
    let p = wasm::definition(manifest("Pushy"), wat.as_bytes(), Limits::default()).expect_err("refused");
    assert!(p.message.contains("faber:capabilities/motor@0.1.0#set-duty"), "{p:?}");
}

#[test]
fn a_file_that_is_not_a_connector_is_refused_with_a_hint() {
    for bytes in [&b"not wasm"[..], b"(component)"] {
        let p = wasm::definition(manifest("Odd"), bytes, Limits::default()).expect_err("refused");
        assert!(p.message.starts_with("Odd can't be loaded:"), "{p:?}");
        assert!(p.hint.contains("faber:connector@0.1.0"), "{p:?}");
    }
}

mod supervised {
    //! The same sandboxed connector under the real supervisor: a crash restarts it with a fresh
    //! instance, exactly like a built-in connector.

    use super::*;
    use faber_core::clock::MonotonicClock;
    use faber_core::names::PartName;
    use faber_core::ports::{PortEntry, PortId, PortKind, PortOpener, PortReader, PortWriter};
    use faber_core::router::Router;
    use faber_core::runtime::{self, PartStatus, RunDeps, RunSpec, StatusBoard, Timing};
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Every time the port opens it sends `bytes`, then stays silent.
    struct Port {
        bytes: Vec<u8>,
        opens: AtomicU32,
    }

    struct Reader(Vec<u8>);
    impl PortReader for Reader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                std::thread::sleep(Duration::from_millis(5));
                return Ok(0);
            }
            let n = self.0.len().min(buf.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0.drain(..n);
            Ok(n)
        }
    }

    struct Sink;
    impl PortWriter for Sink {
        fn write(&mut self, _: &[u8]) -> io::Result<()> {
            Ok(())
        }
    }

    impl PortOpener for Port {
        fn open(&self, _: &PortEntry, _: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok((Box::new(Reader(self.bytes.clone())), Box::new(Sink)))
        }
    }

    fn wait_for(status: &StatusBoard, name: &PartName, f: impl Fn(&PartStatus) -> bool) -> PartStatus {
        let start = Instant::now();
        loop {
            if let Some(s) = status.get(name).filter(|s| f(s)) {
                return s;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "timed out; last {:?}", status.get(name));
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_crashed_wasm_connector_restarts_with_a_fresh_instance_then_stops() {
        let Some(wasm) = component("faber-test-guest") else { return };
        let def = wasm::definition(faber_test_guest::manifest(), &wasm, Limits::default()).unwrap();
        let port = Arc::new(Port {
            bytes: vec![faber_test_guest::byte::PUBLISH, faber_test_guest::byte::PANIC],
            opens: AtomicU32::new(0),
        });
        let deps = RunDeps {
            clock: Arc::new(MonotonicClock::new()),
            opener: Arc::clone(&port) as Arc<dyn PortOpener>,
            router: Arc::new(Router::new()),
            status: Arc::new(StatusBoard::default()),
            timing: Timing {
                tick: Duration::from_millis(5),
                backoff_start: Duration::from_millis(5),
                port_retry_max: Duration::from_millis(20),
                max_crashes: 3,
                healthy_after: Duration::from_secs(30),
            },
        };
        let name = PartName::parse("guest").unwrap();
        let entry = PortEntry {
            id: PortId::Serial("FAKE".into()),
            kind: PortKind::Serial,
            label: "FAKE".into(),
            path: Some("FAKE".into()),
            usb: None,
        };
        let _part = runtime::start(RunSpec::new(name.clone(), &def, Settings::new(), entry), deps.clone());
        let PartStatus::Stopped(p) = wait_for(&deps.status, &name, |s| matches!(s, PartStatus::Stopped(_)))
        else {
            unreachable!()
        };
        assert!(p.message.contains("crashed 3 times"), "{p:?}");
        assert_eq!(port.opens.load(Ordering::SeqCst), 3, "a fresh instance each time, up to the limit");
    }
}

mod installed {
    //! Connectors installed in `<data>/connectors/<id>/` (faber.toml + connector.wasm) are offered
    //! next to the built-in ones, as `kind: "wasm"`, and run parts like them. Bad folders are
    //! reported, never fatal.

    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use faber_core::api;
    use faber_core::clock::MonotonicClock;
    use faber_core::disks::{Disk, DiskDiscovery};
    use faber_core::logs::LogBook;
    use faber_core::machine::{Machine, MachineOptions};
    use faber_core::ports::{
        Discovered, FailedUsb, ReplayLibrary, SerialDiscovery, SystemOpener, UsbProblemDiscovery,
    };
    use faber_core::runtime::Timing;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tower::ServiceExt;

    struct Nothing;
    impl SerialDiscovery for Nothing {
        fn list(&self) -> Vec<Discovered> {
            Vec::new()
        }
    }
    impl UsbProblemDiscovery for Nothing {
        fn list(&self) -> Vec<FailedUsb> {
            Vec::new()
        }
    }
    impl DiskDiscovery for Nothing {
        fn list(&self) -> Vec<Disk> {
            Vec::new()
        }
    }

    fn start(data: &Path) -> (Arc<Machine>, axum::Router) {
        let clock = Arc::new(MonotonicClock::new());
        let replays = ReplayLibrary::with_demo();
        let machine = Machine::start(MachineOptions {
            data_dir: data.to_path_buf(),
            clock: clock.clone(),
            discovery: Arc::new(Nothing),
            usb_problems: Arc::new(Nothing),
            disks: Arc::new(Nothing),
            opener: Arc::new(SystemOpener::new(replays.clone())),
            replays,
            connectors: faber_core::connectors::built_in(),
            timing: Timing::default(),
            port_scan: None,
            world: Arc::new(faber_core::simparts::SimWorld::new()),
        })
        .unwrap();
        let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
        (machine, app)
    }

    async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> Value {
        let mut req = Request::builder().method(method).uri(path);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    /// Installs the LD19 component under another id, so it sits next to the built-in LD19.
    fn install_ld19(data: &Path, id: &str, wasm: &[u8]) {
        let dir = data.join("connectors").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let toml = std::fs::read_to_string(workspace().join("connectors/ldrobot-ld19/faber.toml")).unwrap();
        let toml = toml.replace("id = \"ldrobot-ld19\"", &format!("id = \"{id}\""));
        assert!(toml.starts_with("faber-manifest = 1"), "every faber.toml says its format first");
        std::fs::write(dir.join("faber.toml"), toml).unwrap();
        std::fs::write(dir.join("connector.wasm"), wasm).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_installed_connector_is_offered_as_wasm_and_runs_a_part() {
        let Some(wasm) = component("ldrobot-ld19") else { return };
        let data = tempfile::tempdir().unwrap();
        install_ld19(data.path(), "ld19-wasm", &wasm);
        let (machine, app) = start(data.path());

        let connectors = call(&app, "GET", "/api/v0/connectors", None).await;
        let kinds: Vec<(&str, &str)> = connectors
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["id"].as_str().unwrap(), c["kind"].as_str().unwrap()))
            .collect();
        assert!(kinds.contains(&("ldrobot-ld19", "built-in")), "{kinds:?}");
        assert!(kinds.contains(&("ld19-wasm", "wasm")), "{kinds:?}");
        assert_eq!(call(&app, "GET", "/api/v0/connectors/problems", None).await, json!([]));

        let part = json!({"name": "lidar", "connector": "ld19-wasm", "port": "replay:demo-ld19"});
        call(&app, "POST", "/api/v0/parts", Some(part)).await;
        let started = Instant::now();
        loop {
            let parts = call(&app, "GET", "/api/v0/parts", None).await;
            if parts[0]["state"]["kind"] == "running" {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "never ran: {parts}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let scan = call(&app, "GET", "/api/v0/signals/lidar/laser-scan", None).await;
        assert_eq!(scan["capability"], "laser-scan@1", "{scan}");
        machine.shutdown();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn bad_connector_folders_are_reported_and_the_core_runs_without_them() {
        let data = tempfile::tempdir().unwrap();
        let broken = data.path().join("connectors/broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("faber.toml"), "faber-manifest = 1\n[package]\nkind = \"wasm\"\nid = 3\n")
            .unwrap();
        if let Some(wasm) = component("ldrobot-ld19") {
            install_ld19(data.path(), "ldrobot-ld19", &wasm);
        }
        let (machine, app) = start(data.path());

        let problems = call(&app, "GET", "/api/v0/connectors/problems", None).await;
        let problems = problems.as_array().unwrap();
        assert_eq!(problems[0]["folder"], "broken");
        assert!(
            problems[0]["message"].as_str().unwrap().contains("faber.toml can't be read"),
            "{problems:?}"
        );
        assert!(!problems[0]["hint"].as_str().unwrap().is_empty());
        if problems.len() > 1 {
            assert_eq!(problems[1]["folder"], "ldrobot-ld19");
            assert!(problems[1]["message"].as_str().unwrap().contains("already there"));
        }
        let connectors = call(&app, "GET", "/api/v0/connectors", None).await;
        let ld19: Vec<_> =
            connectors.as_array().unwrap().iter().filter(|c| c["id"] == "ldrobot-ld19").collect();
        assert_eq!(ld19.len(), 1, "the built-in one stays, alone");
        assert_eq!(ld19[0]["kind"], "built-in");
        machine.shutdown();
    }
}
