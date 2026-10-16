//! What the app tests share (`tests/apps.rs`, `tests/bridge.rs`): the example apps built as
//! components from the workspace's own crates, and packed as `faber pack` packs them.
//!
//! Without the `wasm32-unknown-unknown` target the tests say so and pass; CI sets
//! `FABER_REQUIRE_WASM=1`, which makes a missing target a failure.

#![allow(dead_code)] // each test binary uses only some of this

use faber_core::packages;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Mutex;

const TARGET: &str = "wasm32-unknown-unknown";

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn have_target() -> bool {
    let rustc = Path::new(env!("CARGO")).with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    let rustc = if rustc.exists() { rustc } else { PathBuf::from("rustc") };
    let Ok(out) = Process::new(rustc).args(["--print", "target-libdir", "--target", TARGET]).output() else {
        return false;
    };
    let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    out.status.success() && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}

/// The component built from a workspace crate, or `None` (with a message) without the target.
pub fn component(package: &str) -> Option<Vec<u8>> {
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
            "skipping the app tests: the {TARGET} target is not installed (rustup target add {TARGET})."
        );
        return None;
    }
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

/// An example app (`examples/apps/<name>`), packed as `faber pack` packs it: its manifest,
/// README, LICENSE, every file of its UI and its component.
pub fn example_package(name: &str, wasm: &[u8]) -> Vec<u8> {
    let dir = workspace().join("examples/apps").join(name);
    let mut files = BTreeMap::new();
    for f in ["faber.toml", "README.md", "LICENSE"] {
        files.insert(f.to_string(), std::fs::read(dir.join(f)).unwrap());
    }
    for entry in std::fs::read_dir(dir.join("ui")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        files.insert(format!("ui/{name}"), std::fs::read(entry.path()).unwrap());
    }
    files.insert("app.wasm".to_string(), wasm.to_vec());
    packages::pack(&files).unwrap()
}

/// A package's files by name, all but its component.
fn files_but_the_component(package: &[u8]) -> BTreeMap<String, Vec<u8>> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(package)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).unwrap();
        let name = e.name().unwrap().into_owned();
        if !name.ends_with(".wasm") {
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            out.insert(name, data);
        }
    }
    out
}

/// The package a shell end-to-end test installs (`shell/e2e/fixtures/<file>`) must be `fresh`.
/// Written here and committed, like the generated TypeScript types. Every file but the component
/// must match; the component's bytes depend on the Rust toolchain (CI's stable moves on), so it is
/// only rewritten with FABER_REFRESH_FIXTURES set, or with the rest.
pub fn check_e2e_fixture(file: &str, fresh: Vec<u8>) {
    let committed = workspace().join("shell/e2e/fixtures").join(file);
    let old = std::fs::read(&committed).ok();
    let refresh = std::env::var_os("FABER_REFRESH_FIXTURES").is_some();
    let same = old.as_deref().is_some_and(|o| files_but_the_component(o) == files_but_the_component(&fresh));
    if !same || (refresh && old.as_deref() != Some(&fresh)) {
        std::fs::create_dir_all(committed.parent().unwrap()).unwrap();
        std::fs::write(&committed, fresh).unwrap();
        panic!("{} was out of date and has been rewritten; commit it", committed.display());
    }
    packages::open(&old.unwrap()).expect("the committed package opens");
}

/// An app's window as the shell runs it for its page (docs/apps.md §12): a WebSocket to the core
/// with the window's link to its app's logic open, its heartbeat every 100 ms, and what the core
/// tells it kept in order. Dropping it is the window closing (the connection goes).
pub struct AppWindow {
    out: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
    heard: std::sync::Arc<Mutex<Vec<serde_json::Value>>>,
    beat: Option<tokio::task::JoinHandle<()>>,
    hold: Option<tokio::task::JoinHandle<()>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// The window's link id on its WebSocket.
const LINK: u32 = 1;

impl AppWindow {
    /// Opens `app`'s window on the core behind `router`, and says hello to its logic.
    pub async fn open(router: &axum::Router, app: &str) -> AppWindow {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = router.clone();
        let serve = tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });
        let (ws, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
        let (mut sink, mut stream) = ws.split();
        let (out, mut rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
        let writer = tokio::spawn(async move {
            while let Some(m) = rx.recv().await {
                if sink.send(Message::Text(m.to_string().into())).await.is_err() {
                    return;
                }
            }
        });
        let heard = std::sync::Arc::new(Mutex::new(Vec::new()));
        let h = std::sync::Arc::clone(&heard);
        let reader = tokio::spawn(async move {
            while let Some(Ok(m)) = stream.next().await {
                if let Message::Text(t) = m {
                    h.lock().unwrap().push(serde_json::from_str(&t).unwrap());
                }
            }
        });
        let mut w = AppWindow { out, heard, beat: None, hold: None, tasks: vec![serve, writer, reader] };
        w.say(serde_json::json!({"type": "app-window-open", "id": LINK, "app": app}));
        let opened = w.wait_for("the link to open", |m| m["type"] != "app-window-message").await;
        assert_eq!(opened["type"], "app-window-opened", "{opened}");
        let out = w.out.clone();
        w.beat = Some(tokio::spawn(async move {
            loop {
                let _ = out.send(serde_json::json!({"type": "app-window-heartbeat", "id": LINK}));
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }));
        w.send("hello", "");
        w
    }

    fn say(&self, m: serde_json::Value) {
        self.out.send(m).unwrap();
    }

    /// A message for the app's logic.
    pub fn send(&self, kind: &str, text: &str) {
        self.say(
            serde_json::json!({"type": "app-window-send", "id": LINK, "kind": kind, "data": {"text": text}}),
        );
    }

    /// Everything the core told the window so far.
    pub fn heard(&self) -> Vec<serde_json::Value> {
        self.heard.lock().unwrap().clone()
    }

    /// The first thing the core told the window that `want` says yes to, waiting up to 5 s.
    pub async fn wait_for(&self, what: &str, want: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(m) = self.heard().into_iter().find(|m| want(m)) {
                return m;
            }
            assert!(std::time::Instant::now() < until, "timed out waiting for {what}: {:?}", self.heard());
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    /// Drives `role` through the logic, as the shell does once the user said yes (D46).
    pub async fn drive_via(&self, role: &str) {
        let before = self.heard().len();
        self.say(serde_json::json!({"type": "app-window-drive", "id": LINK, "role": role}));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let answer = self
                .heard()
                .into_iter()
                .skip(before)
                .find(|m| m["type"] == "app-window-driving" || m["type"] == "app-window-drive-refused");
            if let Some(a) = answer {
                assert_eq!(a["type"], "app-window-driving", "{a}");
                return;
            }
            assert!(std::time::Instant::now() < until, "no answer to app-window-drive");
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    /// What the window's page does while a control is held: `drive` every 100 ms.
    pub fn hold(&mut self, x: f32, y: f32, turn: f32) {
        if let Some(t) = self.hold.take() {
            t.abort();
        }
        let out = self.out.clone();
        self.hold = Some(tokio::spawn(async move {
            loop {
                let m = serde_json::json!({"type": "app-window-send", "id": LINK, "kind": "drive", "data": {"text": format!("{x} {y} {turn}")}});
                let _ = out.send(m);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }));
    }

    /// The control is let go: one last `drive` for nothing, and the drive is released.
    pub fn let_go(&mut self, role: &str) {
        if let Some(t) = self.hold.take() {
            t.abort();
        }
        self.send("drive", "0 0 0");
        self.say(serde_json::json!({"type": "app-window-release", "id": LINK, "role": role}));
    }

    /// The page hangs: nothing more comes from it, not its messages, not its heartbeat.
    pub fn hang(&mut self) {
        for t in [self.hold.take(), self.beat.take()].into_iter().flatten() {
            t.abort();
        }
    }

    /// The user presses the shell's Stop bar for this window.
    pub fn press_stop(&self) {
        self.say(serde_json::json!({"type": "app-window-stop", "id": LINK}));
    }

    /// The limits the app's logic last told the window (its `limits` message, parsed).
    pub fn limits(&self) -> serde_json::Value {
        self.heard()
            .into_iter()
            .rev()
            .find(|m| m["type"] == "app-window-message" && m["kind"] == "limits")
            .and_then(|m| serde_json::from_str(m["data"]["text"].as_str()?).ok())
            .unwrap_or(serde_json::Value::Null)
    }
}

impl Drop for AppWindow {
    fn drop(&mut self) {
        for t in [self.hold.take(), self.beat.take()].into_iter().flatten() {
            t.abort();
        }
        for t in &self.tasks {
            t.abort();
        }
    }
}

/// The API as a program signed in with an admin's API token sees it (docs/auth.md). The tests go
/// through the real sign-in path: a first admin, a token, and `Authorization: Bearer` on every
/// request (including WebSocket upgrades) that doesn't carry its own.
pub fn signed_in_api(
    machine: &std::sync::Arc<faber_core::machine::Machine>,
    logs: std::sync::Arc<faber_core::logs::LogBook>,
) -> axum::Router {
    let app = faber_core::api::router(faber_core::api::AppState::new(std::sync::Arc::clone(machine), logs));
    with_token(app, &admin_token(machine))
}

/// Creates the first admin (`tester`) and returns an admin API token of theirs.
pub fn admin_token(machine: &faber_core::machine::Machine) -> String {
    let (admin, _) = machine.create_first_admin("tester", ADMIN_PASSWORD).unwrap();
    machine.create_token("tests", faber_core::users::UserRole::Admin, &admin).unwrap().1
}

/// The test admin's password.
pub const ADMIN_PASSWORD: &str = "test password";

/// `app`, with `Authorization: Bearer <token>` on every request that has none.
pub fn with_token(app: axum::Router, token: &str) -> axum::Router {
    let value = axum::http::HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
    app.layer(axum::middleware::map_request(move |mut req: axum::extract::Request| {
        let value = value.clone();
        async move {
            req.headers_mut().entry(axum::http::header::AUTHORIZATION).or_insert(value);
            req
        }
    }))
}
