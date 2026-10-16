//! The Arduino editor through the HTTP API (docs/arduino-editor.md): installing the tools,
//! sketches from templates, compiling, uploading, and the same-code check.
//!
//! `arduino-cli` is the core's fake ([`faber_core::arduino::fake`]); downloads come from memory.
//! Each board is a simulated bootloader (`faber-flash`'s) and, once a sketch "runs", the
//! FaberLink library's real C++ core (`faberlink_interop::FakeBoard`), behind a fake serial port.
//! NEVER a real port, and nothing is downloaded.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_core::api;
use faber_core::arduino::fake::{FakeFinder, FakeToolchain};
use faber_core::arduino::install::{Downloader, Release};
use faber_core::clock::{MonotonicClock, SharedClock};
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions, ToolsProgress};
use faber_core::ports::{
    Discovered, FailedUsb, PortEntry, PortOpener, PortReader, PortWriter, ReplayLibrary, SerialDiscovery,
    Usb, UsbProblemDiscovery,
};
use faber_core::runtime::Timing;
use faber_flash::board::{MEGA2560, UNO};
use faber_flash::sim::SimBoard;
use faber_flash::{Board, Image};
use faberlink_interop::FakeBoard;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// What runs on a board after its bootloader: a FaberLink sketch with (or without) a build id,
/// or anything else (nothing on the wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sketch {
    Silent,
    FaberLink { build_id: Option<u64> },
}

/// One board on its USB cable: the bootloader, and the sketch it starts.
struct Cable {
    path: &'static str,
    boot: Mutex<SimBoard>,
    sketch: Mutex<(Sketch, Option<FakeBoard>)>,
    start: Instant,
}

impl Cable {
    fn new(path: &'static str, board: &Board) -> Arc<Cable> {
        Arc::new(Cable {
            path,
            boot: Mutex::new(SimBoard::new(board)),
            sketch: Mutex::new((Sketch::Silent, None)),
            start: Instant::now(),
        })
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// The board now runs this sketch (as after an upload, or after the IDE flashed it).
    fn runs(&self, s: Sketch) {
        *self.sketch.lock().unwrap() = (s, start(s));
    }

    fn flash(&self) -> Vec<u8> {
        self.boot.lock().unwrap().flash().to_vec()
    }

    fn resets(&self) -> u32 {
        self.boot.lock().unwrap().resets()
    }

    fn pages_written(&self) -> usize {
        self.boot.lock().unwrap().pages_written()
    }
}

fn start(s: Sketch) -> Option<FakeBoard> {
    match s {
        Sketch::Silent => None,
        Sketch::FaberLink { build_id } => {
            let mut b = FakeBoard::new(250);
            if let Some(id) = build_id {
                b.set_build_id(id);
            }
            Some(b)
        }
    }
}

struct End(Arc<Cable>);

impl PortReader for End {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(1));
        let now = self.0.now();
        let mut out = self.0.boot.lock().unwrap().take_output(now);
        if let (_, Some(b)) = &mut *self.0.sketch.lock().unwrap() {
            b.tick(now.as_millis() as u32);
            out.extend(b.take_output());
        }
        let n = out.len().min(buf.len());
        buf[..n].copy_from_slice(&out[..n]);
        Ok(n)
    }
}

impl PortWriter for End {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let now = self.0.now();
        self.0.boot.lock().unwrap().receive(data, now);
        if let (_, Some(b)) = &mut *self.0.sketch.lock().unwrap() {
            b.feed(data, now.as_millis() as u32);
        }
        Ok(())
    }

    fn set_dtr_rts(&mut self, asserted: bool) -> io::Result<()> {
        let now = self.0.now();
        self.0.boot.lock().unwrap().set_dtr_rts(asserted, now);
        if asserted {
            // A reset restarts the sketch: it says nothing until it is spoken to.
            let mut s = self.0.sketch.lock().unwrap();
            s.1 = start(s.0);
        }
        Ok(())
    }
}

struct Opener(Vec<Arc<Cable>>);

impl PortOpener for Opener {
    fn open(&self, port: &PortEntry, _baud: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
        let cable = self
            .0
            .iter()
            .find(|c| port.path.as_deref() == Some(c.path))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not a test port"))?;
        Ok((Box::new(End(Arc::clone(cable))), Box::new(End(Arc::clone(cable)))))
    }
}

struct Ports;

fn usb(path: &str, vid: u16, pid: u16, serial: Option<&str>) -> Discovered {
    Discovered {
        path: path.into(),
        usb: Some(Usb {
            vid,
            pid,
            serial_number: serial.map(str::to_string),
            manufacturer: None,
            product: Some("Board".into()),
        }),
        bluetooth: false,
    }
}

impl SerialDiscovery for Ports {
    fn list(&self) -> Vec<Discovered> {
        vec![
            usb("UNO", 0x2341, 0x0043, Some("75830333238351F0E1A1")),
            // Windows makes up a "serial number" for a CH340 from where it is plugged in.
            usb("CLONE", 0x1a86, 0x7523, Some("5&2C3D4E&0&3")),
            usb("MEGA", 0x2341, 0x0042, Some("MEGA-1")),
        ]
    }
}
impl UsbProblemDiscovery for Ports {
    fn list(&self) -> Vec<FailedUsb> {
        Vec::new()
    }
}
impl DiskDiscovery for Ports {
    fn list(&self) -> Vec<Disk> {
        Vec::new()
    }
}

const UNO_PORT: &str = "usb:2341:0043";
const CLONE_PORT: &str = "usb:1a86:7523";
const MEGA_PORT: &str = "usb:2341:0042";

struct TestCore {
    app: axum::Router,
    machine: Arc<Machine>,
    cli: Arc<FakeToolchain>,
    uno: Arc<Cable>,
    clone: Arc<Cable>,
    mega: Arc<Cable>,
    dir: tempfile::TempDir,
}

impl Drop for TestCore {
    fn drop(&mut self) {
        self.machine.shutdown();
    }
}

/// A core whose `arduino-cli` is ready.
fn core() -> TestCore {
    let c = core_without_tools();
    c.machine.use_arduino_finder(Arc::new(FakeFinder { toolchain: Arc::clone(&c.cli), installed_in: None }));
    c.cli.install_avr_core_now();
    c
}

/// A core with no `arduino-cli` until "Install Arduino tools" puts one in its tools folder.
fn core_without_tools() -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let (uno, clone, mega) =
        (Cable::new("UNO", &UNO), Cable::new("CLONE", &UNO), Cable::new("MEGA", &MEGA2560));
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: Arc::clone(&clock),
        discovery: Arc::new(Ports),
        usb_problems: Arc::new(Ports),
        disks: Arc::new(Ports),
        opener: Arc::new(Opener(vec![Arc::clone(&uno), Arc::clone(&clone), Arc::clone(&mega)])),
        replays: ReplayLibrary::default(),
        connectors: faber_core::connectors::built_in(),
        timing: Timing {
            backoff_start: Duration::from_millis(10),
            port_retry_max: Duration::from_millis(50),
            ..Timing::default()
        },
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    let cli = FakeToolchain::new(false);
    machine.use_arduino_finder(Arc::new(FakeFinder {
        toolchain: Arc::clone(&cli),
        installed_in: Some(dir.path().to_path_buf()),
    }));
    let app = api::router(api::AppState { machine: Arc::clone(&machine), logs: LogBook::new(clock) });
    TestCore { app, machine, cli, uno, clone, mega, dir }
}

trait InstallNow {
    fn install_avr_core_now(&self);
}

impl InstallNow for FakeToolchain {
    fn install_avr_core_now(&self) {
        use faber_core::arduino::Toolchain;
        self.install_avr_core(&mut |_| {}).unwrap();
    }
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

impl TestCore {
    fn local(&self, path: &str) -> std::path::PathBuf {
        let rel = path.strip_prefix("home:/").unwrap();
        self.dir.path().join("files").join(rel)
    }

    async fn new_sketch(&self, name: &str, template: &str) -> String {
        let (s, v) = call(
            &self.app,
            "POST",
            "/api/v0/arduino/sketches",
            Some(json!({"name": name, "template": template})),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
        v["path"].as_str().unwrap().to_string()
    }

    async fn upload(&self, sketch: &str, board: &str, port: &str) -> Value {
        let (s, v) = call(
            &self.app,
            "POST",
            "/api/v0/arduino/upload",
            Some(json!({"sketch": sketch, "board": board, "port": port})),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v
    }

    async fn code(&self, port: &str) -> Value {
        let (s, v) = call(&self.app, "GET", &format!("/api/v0/arduino/boards/{port}/code"), None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v
    }

    async fn check(&self, port: &str, body: Value) -> (StatusCode, Value) {
        call(&self.app, "POST", &format!("/api/v0/arduino/boards/{port}/check"), Some(body)).await
    }

    /// Waits for the bridge `name` to have an answer from its board.
    async fn wait_for_board(&self, name: &str) -> Value {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let (_, parts) = call(&self.app, "GET", "/api/v0/parts", None).await;
            let p = parts.as_array().unwrap().iter().find(|p| p["name"] == name).cloned();
            if let Some(p) = p
                && p["board"]["link"] == "ready"
            {
                return p;
            }
            assert!(Instant::now() < until, "the board never answered: {parts}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Installing the tools

/// Serves arduino-cli's release for this computer from memory: an archive holding `program`, and
/// the published checksums.
struct Published {
    files: BTreeMap<String, Vec<u8>>,
}

impl Downloader for Published {
    fn download(
        &self,
        url: &str,
        to: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), String> {
        let body = self.files.get(url).ok_or("404 Not Found")?;
        progress(body.len() as u64, Some(body.len() as u64));
        std::fs::write(to, body).map_err(|e| e.to_string())
    }
}

fn published(program: &[u8]) -> (Published, Release) {
    let release = Release::for_this_computer().unwrap();
    let exe = if cfg!(windows) { "arduino-cli.exe" } else { "arduino-cli" };
    let archive = if release.archive.ends_with(".zip") {
        let mut w = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        w.start_file(exe, zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(program).unwrap();
        w.finish().unwrap().into_inner()
    } else {
        let mut h = [0u8; 512];
        h[..exe.len()].copy_from_slice(exe.as_bytes());
        h[124..136].copy_from_slice(format!("{:011o}\0", program.len()).as_bytes());
        h[156] = b'0';
        let mut tar = h.to_vec();
        tar.extend_from_slice(program);
        tar.resize(tar.len().div_ceil(512) * 512 + 1024, 0);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    };
    let sum: String = Sha256::digest(&archive).iter().map(|b| format!("{b:02x}")).collect();
    let mut files = BTreeMap::new();
    files.insert(release.checksums_url.clone(), format!("{sum}  {}\n", release.archive).into_bytes());
    files.insert(release.archive_url.clone(), archive);
    (Published { files }, release)
}

#[tokio::test]
async fn without_tools_the_editor_offers_to_install_them_then_compiles() {
    let c = core_without_tools();
    let (s, info) = call(&c.app, "GET", "/api/v0/arduino", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(info["tools"]["state"], "missing");
    let release = Release::for_this_computer().unwrap();
    assert_eq!(
        info["tools"]["install"]["url"],
        release.archive_url.as_str(),
        "the confirm shows what is downloaded"
    );
    assert_eq!(info["tools"]["install"]["core"], "arduino:avr");
    assert_eq!(info["tools"]["install"]["approxMb"], 150);
    assert_eq!(info["boards"].as_array().unwrap().len(), 4);
    assert_eq!(info["sketchbook"], "home:/Arduino");

    let sketch = c.new_sketch("Blink", "blank").await;
    let (s, e) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": sketch, "board": "uno"})))
            .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("arduino-tools-missing")));
    assert!(e["error"]["hint"].as_str().unwrap().contains("Install Arduino tools"));

    let (d, _) = published(b"arduino-cli, as published");
    c.machine.use_arduino_downloader(Arc::new(d));
    let mut events = c.machine.arduino_events();
    let (s, tools) = call(&c.app, "POST", "/api/v0/arduino/tools/install", None).await;
    assert_eq!(s, StatusCode::OK, "{tools}");
    assert_eq!(tools["state"], "ready");
    assert_eq!(tools["cli"]["source"], "fake");
    let exe = c.dir.path().join("tools/arduino-cli").join(if cfg!(windows) {
        "arduino-cli.exe"
    } else {
        "arduino-cli"
    });
    assert_eq!(std::fs::read(exe).unwrap(), b"arduino-cli, as published");
    let mut steps = Vec::new();
    while let Ok(e) = events.try_recv() {
        steps.push(e);
    }
    assert!(matches!(steps[0], ToolsProgress::Downloading { .. }), "{steps:?}");
    assert!(steps.contains(&ToolsProgress::Verifying) && steps.contains(&ToolsProgress::Unpacking));
    assert!(steps.iter().any(|s| matches!(s, ToolsProgress::InstallingCore { .. })));
    assert_eq!(steps.last(), Some(&ToolsProgress::Done));

    let (s, r) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": sketch, "board": "uno"})))
            .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["ok"], true);
}

#[tokio::test]
async fn a_download_that_isnt_the_published_file_installs_nothing() {
    let c = core_without_tools();
    let (mut d, release) = published(b"x");
    d.files.insert(release.archive_url.clone(), b"something else".to_vec());
    c.machine.use_arduino_downloader(Arc::new(d));
    let (s, e) = call(&c.app, "POST", "/api/v0/arduino/tools/install", None).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("checksum-mismatch")),
        "{e}"
    );
    let (_, info) = call(&c.app, "GET", "/api/v0/arduino", None).await;
    assert_eq!(info["tools"]["state"], "missing");
}

// ---------------------------------------------------------------------------------------------
// Sketches and compiling

#[tokio::test]
async fn a_sketch_from_a_template_compiles_with_its_size_and_the_faberlink_library() {
    let c = core();
    let path = c.new_sketch("Rover", "faberlink-board").await;
    assert_eq!(path, "home:/Arduino/Rover/Rover.ino");
    let text = std::fs::read_to_string(c.local(&path)).unwrap();
    assert!(text.contains("link.begin(115200, \"rover\""), "{text}");

    let (s, r) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": path, "board": "uno"}))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["ok"], true);
    assert_eq!(r["sketchName"], "Rover");
    assert_eq!(r["size"]["flashMax"], 32256);
    assert!(r["size"]["flash"].as_u64().unwrap() > 0);
    assert_eq!(r["diagnostics"], json!([]), "the Arduino core's own warnings are left out");
    let build_id = r["buildId"].as_str().unwrap();
    assert_eq!(build_id.len(), 16);

    let job = c.cli.last_job.lock().unwrap().clone().unwrap();
    assert_eq!(format!("{:016x}", job.build_id), build_id, "compiled with the build id it reports");
    assert_eq!(job.fqbn, "arduino:avr:uno");
    let lib = &job.libraries[0];
    assert!(lib.ends_with("FaberLink") && lib.join("src/FaberLink.h").is_file(), "{lib:?}");
    assert!(lib.join("src/FaberLinkCore.cpp").is_file());

    // Any file of the sketch, or its folder, compiles the sketch.
    for p in ["home:/Arduino/Rover", "home:/Arduino/Rover/Rover.ino"] {
        let (s, r2) =
            call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": p, "board": "uno"}))).await;
        assert_eq!((s, r2["buildId"].as_str()), (StatusCode::OK, Some(build_id)));
    }
    // Another board is another build.
    let (_, mega) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": path, "board": "mega2560"})))
            .await;
    assert_ne!(mega["buildId"].as_str(), Some(build_id));
    assert_eq!(mega["size"]["flashMax"], 253952);
}

#[tokio::test]
async fn new_sketch_names_follow_arduinos_rules_and_never_overwrite() {
    let c = core();
    c.new_sketch("Blink", "blank").await;
    let (s, e) =
        call(&c.app, "POST", "/api/v0/arduino/sketches", Some(json!({"name": "Blink", "template": "blank"})))
            .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("already-exists")));
    let (s, e) = call(
        &c.app,
        "POST",
        "/api/v0/arduino/sketches",
        Some(json!({"name": "my sketch", "template": "blank"})),
    )
    .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid-sketch-name")));
    let (s, e) =
        call(&c.app, "POST", "/api/v0/arduino/sketches", Some(json!({"name": "X", "template": "nope"})))
            .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("unknown-template")));
}

#[tokio::test]
async fn compile_errors_come_back_as_file_and_line() {
    let c = core();
    let path = c.new_sketch("Broken", "blank").await;
    let dir = c.local(&path).parent().unwrap().to_path_buf();
    std::fs::write(dir.join("motor.h"), "#pragma once\n\n#error the motor isn't wired yet\n").unwrap();
    let (s, r) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": path, "board": "uno"}))).await;
    assert_eq!(s, StatusCode::OK, "a sketch with errors is an answer, not a failure");
    assert_eq!(r["ok"], false);
    assert_eq!(
        r["diagnostics"],
        json!([{
            "severity": "error",
            "file": "motor.h",
            "inSketch": true,
            "line": 3,
            "column": 2,
            "message": "#error the motor isn't wired yet"
        }])
    );
    // A folder that isn't a sketch says what a sketch is.
    std::fs::create_dir_all(c.dir.path().join("files/Documents/notes")).unwrap();
    std::fs::write(c.dir.path().join("files/Documents/notes/a.ino"), "").unwrap();
    let (s, e) = call(
        &c.app,
        "POST",
        "/api/v0/arduino/compile",
        Some(json!({"sketch": "home:/Documents/notes/a.ino", "board": "uno"})),
    )
    .await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("not-a-sketch")));
}

// ---------------------------------------------------------------------------------------------
// Uploading and the same-code check

#[tokio::test]
async fn an_upload_is_recorded_with_an_exact_copy_of_what_went_on_the_board() {
    let c = core();
    let path = c.new_sketch("Blink", "blank").await;
    let r = c.upload(&path, "uno", UNO_PORT).await;
    assert_eq!(r["compile"]["ok"], true);
    assert_eq!(r["flashed"]["board"], "uno");
    let record = &r["record"];
    assert_eq!(record["sketch"], path.as_str());
    assert_eq!(
        record["identity"],
        json!({"serial": "75830333238351F0E1A1", "port": UNO_PORT, "unique": true})
    );
    assert_eq!(record["buildId"], r["compile"]["buildId"]);
    assert!((500..3000).contains(&record["checkMs"].as_u64().unwrap()), "{record}");

    // The program is on the (simulated) board, and the record's hash is of the pages written.
    let flash = c.uno.flash();
    let pages = (record["bytes"].as_u64().unwrap() as usize).div_ceil(128) * 128;
    let sum: String = Sha256::digest(&flash[..pages]).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(record["sha256"], sum.as_str());

    let text = std::fs::read_to_string(c.dir.path().join("arduino/boards.toml")).unwrap();
    assert!(text.contains("serial = \"75830333238351F0E1A1\""), "{text}");
    let copy =
        c.dir.path().join("arduino/flashed").join(record["id"].as_str().unwrap()).join("Blink/Blink.ino");
    assert_eq!(std::fs::read(copy).unwrap(), std::fs::read(c.local(&path)).unwrap());

    // A failed compile flashes nothing.
    std::fs::write(c.local(&path), "#error no\n").unwrap();
    let writes = c.uno.pages_written();
    let r = c.upload(&path, "uno", UNO_PORT).await;
    assert_eq!((r["compile"]["ok"].as_bool(), r.get("flashed")), (Some(false), None));
    assert_eq!(c.uno.pages_written(), writes);
}

#[tokio::test]
async fn reading_back_proves_the_same_code_and_notices_other_code() {
    let c = core();
    let path = c.new_sketch("Blink", "blank").await;
    let up = c.upload(&path, "uno", UNO_PORT).await;
    let id = up["record"]["id"].as_str().unwrap().to_string();

    // Nothing on the port says what runs: only reading back can tell, and that needs asking.
    let code = c.code(UNO_PORT).await;
    assert_eq!(code["code"]["result"], "needs-check");
    assert_eq!(code["code"]["record"]["id"], id.as_str());
    let resets = c.uno.resets();
    assert_eq!(c.uno.resets(), resets, "looking never resets the board");

    let writes = c.uno.pages_written();
    let (s, same) = c.check(UNO_PORT, json!({})).await;
    assert_eq!(s, StatusCode::OK, "{same}");
    assert_eq!(same["code"]["result"], "same");
    assert_eq!(same["code"]["proof"]["method"], "read-back");
    assert!(same["code"]["proof"]["elapsedMs"].as_u64().is_some());
    assert_eq!(c.uno.pages_written(), writes, "reading back writes nothing");
    assert_eq!(c.uno.resets(), resets + 1, "but it resets the board");

    // The board is changed outside the editor (here with the flasher and another program).
    let other = Image::from_bytes(&[0x0C, 0x94, 0x5C, 0x00, 1, 2, 3, 4]).to_hex();
    c.machine
        .write_file("home:/Documents/other.hex", other.as_bytes(), faber_core::files::WriteMode::Replace)
        .unwrap();
    let (s, v) = call(
        &c.app,
        "POST",
        &format!("/api/v0/ports/{UNO_PORT}/flash"),
        Some(json!({"board": "uno", "firmware": {"kind": "file", "path": "home:/Documents/other.hex"}})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, diff) = c.check(UNO_PORT, json!({})).await;
    assert_eq!(diff["code"]["result"], "different");
    assert_eq!(diff["code"]["record"]["sketchName"], "Blink", "the last code Faber flashed");
}

#[tokio::test]
async fn a_board_faber_never_flashed_is_unknown_and_is_not_touched() {
    let c = core();
    assert_eq!(c.code(MEGA_PORT).await["code"]["result"], "never");
    let (s, v) = c.check(MEGA_PORT, json!({})).await;
    assert_eq!((s, v["code"]["result"].as_str()), (StatusCode::OK, Some("never")));
    assert_eq!(c.mega.resets(), 0, "there is nothing to compare with, so nothing is read");
    let (s, e) = call(&c.app, "GET", "/api/v0/arduino/boards/usb:dead:beef/code", None).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("unknown-port")));
}

#[tokio::test]
async fn a_clone_without_a_serial_number_is_known_by_its_port_and_says_so() {
    let c = core();
    let path = c.new_sketch("Blink", "blank").await;
    let r = c.upload(&path, "uno", CLONE_PORT).await;
    assert_eq!(r["record"]["identity"], json!({"port": CLONE_PORT, "unique": false}));
    let code = c.code(CLONE_PORT).await;
    assert_eq!(code["identity"]["unique"], false);
    assert_eq!(code["code"]["result"], "needs-check");
    let (_, same) = c.check(CLONE_PORT, json!({})).await;
    assert_eq!(same["code"]["result"], "same");
    assert_eq!(c.clone.resets(), 2, "one to flash, one to read back");
}

#[tokio::test]
async fn opening_the_code_on_a_board_gives_exactly_what_was_flashed() {
    let c = core();
    let path = c.new_sketch("Blink", "blank").await;
    let flashed = std::fs::read(c.local(&path)).unwrap();
    std::fs::write(c.local(&path).with_file_name("pins.h"), "#define LED 13\n").unwrap();
    let up = c.upload(&path, "uno", UNO_PORT).await;
    let open = format!("/api/v0/arduino/flashed/{}/open", up["record"]["id"].as_str().unwrap());

    // Unchanged in Files: the sketch itself.
    let (s, o) = call(&c.app, "POST", &open, Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert_eq!(o, json!({"path": path, "copy": false}));

    // Edited since: a copy of the flashed files, never the edited ones.
    std::fs::write(c.local(&path), "// edited after the upload\n").unwrap();
    let (_, o) = call(&c.app, "POST", &open, Some(json!({}))).await;
    assert_eq!(o, json!({"path": "home:/Arduino/Blink-on-board/Blink-on-board.ino", "copy": true}));
    let copy = c.local("home:/Arduino/Blink-on-board/Blink-on-board.ino");
    assert_eq!(std::fs::read(&copy).unwrap(), flashed);
    assert_eq!(std::fs::read_to_string(copy.with_file_name("pins.h")).unwrap(), "#define LED 13\n");
    // The copy is a sketch of its own: it compiles.
    let (_, r) =
        call(&c.app, "POST", "/api/v0/arduino/compile", Some(json!({"sketch": o["path"], "board": "uno"})))
            .await;
    assert_eq!(r["ok"], true);
    // Asked for a copy again: the next free name.
    let (_, o) = call(&c.app, "POST", &open, Some(json!({"copy": true}))).await;
    assert_eq!(o["path"], "home:/Arduino/Blink-on-board-2/Blink-on-board-2.ino");
}

#[tokio::test]
async fn a_faberlink_board_proves_its_code_live_with_its_build_id() {
    let c = core();
    let path = c.new_sketch("Rover", "faberlink-board").await;
    let up = c.upload(&path, "uno", UNO_PORT).await;
    let build_id = u64::from_str_radix(up["record"]["buildId"].as_str().unwrap(), 16).unwrap();

    // The board now runs the sketch, which says its build id (the library's C++ core).
    c.uno.runs(Sketch::FaberLink { build_id: Some(build_id) });
    let (s, v) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "rover", "connector": "faber-bridge", "port": UNO_PORT})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let part = c.wait_for_board("rover").await;
    assert_eq!(part["board"]["code"]["buildId"], up["record"]["buildId"]);
    assert_eq!(part["board"]["code"]["record"]["sketchName"], "Rover", "Parts: Code from Faber: Rover");

    let resets = c.uno.resets();
    let code = c.code(UNO_PORT).await;
    assert_eq!(code["code"]["result"], "same");
    assert_eq!(code["code"]["proof"]["method"], "build-id");
    assert_eq!(c.uno.resets(), resets, "the live check resets nothing");

    // Reading back a board in use needs its bridge stopped, and says so.
    let (s, e) = c.check(UNO_PORT, json!({})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("port-used-by-bridge")));
    assert!(e["error"]["hint"].as_str().unwrap().starts_with("Checking the code stops \"rover\""), "{e}");
    let (s, v) = c.check(UNO_PORT, json!({"stopPart": true})).await;
    assert_eq!((s, v["code"]["result"].as_str()), (StatusCode::OK, Some("same")), "{v}");
    c.wait_for_board("rover").await;

    // The Arduino IDE flashes the board: the same FaberLink sketch, but no build id.
    let (s, _) = call(&c.app, "DELETE", "/api/v0/parts/rover", None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    c.uno.runs(Sketch::FaberLink { build_id: None });
    call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "rover", "connector": "faber-bridge", "port": UNO_PORT})),
    )
    .await;
    let part = c.wait_for_board("rover").await;
    assert_eq!(part["board"]["code"], json!({}), "Parts: Code not flashed from Faber");
    let code = c.code(UNO_PORT).await;
    assert_eq!(code["code"]["result"], "different");
    assert_eq!(code["code"]["record"]["sketchName"], "Rover");
}

#[tokio::test]
async fn uploading_to_a_board_in_use_needs_its_bridge_stopped() {
    let c = core();
    let path = c.new_sketch("Rover", "faberlink-board").await;
    c.uno.runs(Sketch::FaberLink { build_id: None });
    call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "rover", "connector": "faber-bridge", "port": UNO_PORT})),
    )
    .await;
    c.wait_for_board("rover").await;
    let body = json!({"sketch": path, "board": "uno", "port": UNO_PORT});
    let (s, e) = call(&c.app, "POST", "/api/v0/arduino/upload", Some(body.clone())).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("port-used-by-bridge")));
    let mut body = body;
    body["stopPart"] = json!(true);
    let (s, r) = call(&c.app, "POST", "/api/v0/arduino/upload", Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["flashed"]["stoppedPart"], "rover");
}
