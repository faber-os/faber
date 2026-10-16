//! The Arduino flasher through the HTTP API (docs/api.md, "The Arduino flasher"). The boards are
//! `faber-flash`'s simulated bootloaders behind fake serial ports: an official Uno, a CH340
//! clone and a Mega. NEVER a real port: the opener below is the only one this core has.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use faber_core::clock::{MonotonicClock, SharedClock};
use faber_core::disks::{Disk, DiskDiscovery};
use faber_core::logs::LogBook;
use faber_core::machine::{Machine, MachineOptions};
use faber_core::packages;
use faber_core::ports::{
    Discovered, FailedUsb, PortEntry, PortOpener, PortReader, PortWriter, ReplayLibrary, SerialDiscovery,
    Usb, UsbProblemDiscovery,
};
use faber_flash::board::{MEGA2560, UNO};
use faber_flash::sim::{Faults, SimBoard};
use faber_flash::{Image, Stage};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// One board on its USB cable, and what happened on it.
struct Cable {
    board: Mutex<SimBoard>,
    start: Instant,
    /// Every open, by baud.
    opens: Mutex<Vec<u32>>,
    /// Who held the port when the flasher reset the board.
    claimed_at_reset: Mutex<Option<String>>,
    path: &'static str,
    machine: OnceLock<Weak<Machine>>,
}

impl Cable {
    fn new(path: &'static str, board: SimBoard) -> Arc<Cable> {
        Arc::new(Cable {
            board: Mutex::new(board),
            start: Instant::now(),
            opens: Mutex::new(Vec::new()),
            claimed_at_reset: Mutex::new(None),
            path,
            machine: OnceLock::new(),
        })
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn flash(&self) -> Vec<u8> {
        self.board.lock().unwrap().flash().to_vec()
    }

    fn resets(&self) -> u32 {
        self.board.lock().unwrap().resets()
    }
}

struct End(Arc<Cable>);

impl PortReader for End {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let now = self.0.now();
        let out = self.0.board.lock().unwrap().take_output(now);
        // The simulated bootloader answers at once; a real port would hand over what fits.
        assert!(out.len() <= buf.len());
        buf[..out.len()].copy_from_slice(&out);
        Ok(out.len())
    }
}

impl PortWriter for End {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let now = self.0.now();
        self.0.board.lock().unwrap().receive(data, now);
        Ok(())
    }

    fn set_dtr_rts(&mut self, asserted: bool) -> io::Result<()> {
        if asserted && let Some(m) = self.0.machine.get().and_then(Weak::upgrade) {
            let holder = m.ports().into_iter().find(|p| p.entry.path.as_deref() == Some(self.0.path));
            *self.0.claimed_at_reset.lock().unwrap() = holder.and_then(|p| p.claimed_by);
        }
        let now = self.0.now();
        self.0.board.lock().unwrap().set_dtr_rts(asserted, now);
        Ok(())
    }
}

struct Opener(Vec<Arc<Cable>>);

impl PortOpener for Opener {
    fn open(&self, port: &PortEntry, baud: u32) -> io::Result<(Box<dyn PortReader>, Box<dyn PortWriter>)> {
        let cable = self
            .0
            .iter()
            .find(|c| port.path.as_deref() == Some(c.path))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "not a test port"))?;
        cable.opens.lock().unwrap().push(baud);
        Ok((Box::new(End(Arc::clone(cable))), Box::new(End(Arc::clone(cable)))))
    }
}

struct Ports;

fn usb(path: &str, vid: u16, pid: u16, product: &str) -> Discovered {
    Discovered {
        path: path.into(),
        usb: Some(Usb { vid, pid, serial_number: None, manufacturer: None, product: Some(product.into()) }),
        bluetooth: false,
    }
}

impl SerialDiscovery for Ports {
    fn list(&self) -> Vec<Discovered> {
        vec![
            usb("UNO", 0x2341, 0x0043, "Arduino Uno"),
            usb("CLONE", 0x1a86, 0x7523, "USB Serial"),
            usb("MEGA", 0x2341, 0x0042, "Arduino Mega 2560"),
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

fn core() -> TestCore {
    core_with(SimBoard::new(&UNO))
}

fn core_with(uno: SimBoard) -> TestCore {
    let dir = tempfile::tempdir().unwrap();
    let clock: SharedClock = Arc::new(MonotonicClock::new());
    let (uno, clone, mega) = (
        Cable::new("UNO", uno),
        Cable::new("CLONE", SimBoard::new(&UNO)),
        Cable::new("MEGA", SimBoard::new(&MEGA2560)),
    );
    let machine = Machine::start(MachineOptions {
        data_dir: dir.path().to_path_buf(),
        clock: Arc::clone(&clock),
        discovery: Arc::new(Ports),
        usb_problems: Arc::new(Ports),
        disks: Arc::new(Ports),
        opener: Arc::new(Opener(vec![Arc::clone(&uno), Arc::clone(&clone), Arc::clone(&mega)])),
        replays: ReplayLibrary::default(),
        connectors: faber_core::connectors::built_in(),
        timing: faber_core::runtime::Timing::default(),
        port_scan: None,
        world: Arc::new(faber_core::simparts::SimWorld::new()),
    })
    .unwrap();
    for c in [&uno, &clone, &mega] {
        let _ = c.machine.set(Arc::downgrade(&machine));
    }
    let app = crate::support::signed_in_api(&machine, LogBook::new(clock));
    TestCore { app, machine, uno, clone, mega, dir }
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
    send(app, req.body(body).unwrap()).await
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() })
}

/// A program for the tests: not all one value, so a page in the wrong place shows.
fn program(len: usize) -> Image {
    Image::from_bytes(&(0..len).map(|i| (i * 13 + 7) as u8).collect::<Vec<_>>())
}

fn put_hex(c: &TestCore, name: &str, image: &Image) -> String {
    let docs = c.dir.path().join("files").join("Documents");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join(name), image.to_hex()).unwrap();
    format!("home:/Documents/{name}")
}

fn flash_url(port: &str) -> String {
    format!("/api/v0/ports/{}/flash", port.replace(':', "%3A"))
}

fn file(path: &str) -> Value {
    json!({"kind": "file", "path": path})
}

async fn flash(c: &TestCore, port: &str, body: Value) -> (StatusCode, Value) {
    call(&c.app, "POST", &flash_url(port), Some(body)).await
}

fn assert_on(cable: &Cable, image: &Image) {
    assert_eq!(&cable.flash()[..image.end() as usize], image.bytes(), "the board holds the program");
}

#[tokio::test(flavor = "multi_thread")]
async fn ports_say_which_board_their_usb_ids_name() {
    let c = core();
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    let by_id = |id: &str| ports.as_array().unwrap().iter().find(|p| p["id"] == id).unwrap().clone();
    assert_eq!(by_id(UNO_PORT)["board"], json!({"board": "uno"}));
    assert_eq!(by_id(MEGA_PORT)["board"], json!({"board": "mega2560"}));
    assert_eq!(by_id(CLONE_PORT)["board"], json!({"usbChip": "CH340"}));
    let (_, boards) = call(&c.app, "GET", "/api/v0/flasher/boards", None).await;
    let ids: Vec<&str> = boards.as_array().unwrap().iter().map(|b| b["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["uno", "nano", "nano-old", "mega2560"]);
    assert_eq!(boards[0], json!({"id": "uno", "name": "Arduino Uno", "chip": "ATmega328P", "room": 32256}));
    // Looking at ports opens none of them, and resets no board.
    assert!(c.uno.opens.lock().unwrap().is_empty() && c.uno.resets() == 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_uno_is_flashed_from_a_hex_file_with_progress() {
    let c = core();
    let image = program(3000);
    let path = put_hex(&c, "blink.hex", &image);
    let mut events = c.machine.flash_events();
    let (s, body) = flash(&c, UNO_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"port": UNO_PORT, "board": "uno", "boardName": "Arduino Uno", "bytes": 3000, "pages": 24})
    );
    assert_on(&c.uno, &image);
    assert_eq!(*c.uno.opens.lock().unwrap(), [115_200], "opened once, at the bootloader's speed");
    assert_eq!(c.uno.claimed_at_reset.lock().unwrap().as_deref(), Some("tool:arduino-flasher"));
    let mut stages = Vec::new();
    while let Ok(e) = events.try_recv() {
        assert_eq!(e.port, UNO_PORT);
        stages.push((e.progress.stage, e.progress.done, e.progress.total));
    }
    assert_eq!(stages.first(), Some(&(Stage::Connecting, 0, 3072)));
    assert!(
        stages.contains(&(Stage::Writing, 3072, 3072)) && stages.contains(&(Stage::Verifying, 128, 3072))
    );
    assert_eq!(stages.last(), Some(&(Stage::Done, 3072, 3072)));
    // The port is free again.
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert!(ports.as_array().unwrap().iter().all(|p| p["claimedBy"].is_null()), "{ports}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clone_needs_its_board_picked() {
    let c = core();
    let image = program(500);
    let path = put_hex(&c, "sketch.hex", &image);
    let (s, e) = flash(&c, CLONE_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("board-needed")), "{e}");
    assert!(e["error"]["message"].as_str().unwrap().contains("CH340"), "{e}");
    assert!(c.clone.opens.lock().unwrap().is_empty(), "nothing was opened");
    let (s, body) = flash(&c, CLONE_PORT, json!({"board": "nano", "firmware": file(&path)})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["boardName"], "Arduino Nano");
    assert_on(&c.clone, &image);
    let (s, e) = flash(&c, CLONE_PORT, json!({"board": "due", "firmware": file(&path)})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("unknown-board")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mega_is_flashed_over_stk500v2() {
    let c = core();
    let image = program(70_000);
    let path = put_hex(&c, "big.hex", &image);
    let (s, body) = flash(&c, MEGA_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!((body["board"].as_str(), body["pages"].as_u64()), (Some("mega2560"), Some(274)));
    assert_on(&c.mega, &image);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_firmware_is_refused_before_the_port_is_opened() {
    let c = core();
    let docs = c.dir.path().join("files").join("Documents");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("broken.hex"), ":100000000C945C000C946E000C946E000C946E00CB\n:00000001FF\n")
        .unwrap();
    let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file("home:/Documents/broken.hex")})).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("firmware-invalid"))
    );
    assert!(e["error"]["message"].as_str().unwrap().contains("line 1"), "{e}");
    let big = put_hex(&c, "mega-sketch.hex", &program(40_000));
    let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file(&big)})).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("firmware-too-large"))
    );
    assert_eq!(e["error"]["message"], "The program is 40000 bytes, but the Arduino Uno has room for 32256.");
    let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file("home:/Documents/none.hex")})).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{e}");
    let (s, e) = flash(&c, "serial:NOPE", json!({"firmware": file(&big)})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("unknown-port")));
    assert!(c.uno.opens.lock().unwrap().is_empty() && c.uno.resets() == 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wrong_chip_is_refused_and_nothing_is_written() {
    // An old Nano with an ATmega168 behind the Uno's USB ids.
    let c = core_with(SimBoard::with_chip(faber_flash::Protocol::Stk500v1, [0x1E, 0x94, 0x06], 16_384));
    let path = put_hex(&c, "sketch.hex", &program(500));
    let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("wrong-board")),
        "{e}"
    );
    assert!(e["error"]["message"].as_str().unwrap().contains("ATmega168"), "{e}");
    assert!(c.uno.flash().iter().all(|b| *b == 0xFF), "nothing was written");
    assert_eq!(c.uno.board.lock().unwrap().pages_written(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_board_that_never_answers_says_to_press_reset() {
    let silent = SimBoard::new(&UNO).with_faults(Faults { no_bootloader: true, ..Faults::default() });
    let c = core_with(silent);
    let path = put_hex(&c, "sketch.hex", &program(500));
    let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, Some("board-not-answering"))
    );
    assert_eq!(e["error"]["message"], "The board didn't answer the bootloader.");
    assert!(e["error"]["hint"].as_str().unwrap().starts_with("Try pressing reset just as you press Flash."));
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    assert!(ports.as_array().unwrap().iter().all(|p| p["claimedBy"].is_null()), "the port is given back");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_a_part_uses_is_refused() {
    let c = core();
    let (s, body) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "lidar", "connector": "ldrobot-ld19", "port": UNO_PORT})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let path = put_hex(&c, "sketch.hex", &program(500));
    for stop in [false, true] {
        let (s, e) = flash(&c, UNO_PORT, json!({"firmware": file(&path), "stopPart": stop})).await;
        assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("port-claimed")), "{e}");
        assert!(e["error"]["message"].as_str().unwrap().contains("\"lidar\""), "{e}");
    }
    assert_eq!(c.uno.resets(), 0, "only the part opened the port");
    assert!(!c.uno.opens.lock().unwrap().contains(&115_200));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_bridge_is_stopped_only_when_asked_and_starts_again() {
    let c = core();
    let (s, body) = call(
        &c.app,
        "POST",
        "/api/v0/parts",
        Some(json!({"name": "mega", "connector": "faber-bridge", "port": MEGA_PORT})),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let image = program(2000);
    let path = put_hex(&c, "faberlink.hex", &image);
    let (s, e) = flash(&c, MEGA_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("port-used-by-bridge")), "{e}");
    assert!(e["error"]["hint"].as_str().unwrap().contains("stopPart"), "{e}");
    assert_eq!(c.mega.resets(), 0, "nothing reset the board");

    let (s, body) = flash(&c, MEGA_PORT, json!({"firmware": file(&path), "stopPart": true})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["stoppedPart"], "mega");
    assert_on(&c.mega, &image);
    assert_eq!(c.mega.claimed_at_reset.lock().unwrap().as_deref(), Some("tool:arduino-flasher"));
    // The bridge has its port back and runs again (it looks for a FaberLink board on it).
    let (_, ports) = call(&c.app, "GET", "/api/v0/ports", None).await;
    let mega = ports.as_array().unwrap().iter().find(|p| p["id"] == MEGA_PORT).unwrap().clone();
    assert_eq!(mega["claimedBy"], "mega");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_packages_firmware_is_flashed_for_its_board() {
    let c = core();
    let image = program(1200);
    let toml = "faber-manifest = 1\n[package]\nid = \"my-bridge\"\ntype = \"connector\"\n\
        kind = \"declarative\"\nversion = \"0.1.0\"\nname = \"My bridge\"\n\
        [ports]\nserial = { baud = 115200 }\n[[provides]]\ncapability = \"distance@1\"\n\
        line = { pattern = \"D:{meters:f32}\" }\n[[firmware]]\nboard = \"uno\"\n";
    let files: BTreeMap<String, Vec<u8>> = [
        ("faber.toml", toml.as_bytes().to_vec()),
        ("README.md", b"# My bridge".to_vec()),
        ("LICENSE", b"MIT".to_vec()),
        ("firmware/uno.hex", image.to_hex().into_bytes()),
    ]
    .into_iter()
    .map(|(n, d)| (n.to_string(), d))
    .collect();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v0/packages?name=my-bridge-0.1.0.faberpkg")
        .header("content-type", "application/octet-stream")
        .body(Body::from(packages::pack(&files).unwrap()))
        .unwrap();
    let (s, info) = send(&c.app, req).await;
    assert_eq!(s, StatusCode::CREATED, "{info}");
    assert_eq!(info["firmware"], json!(["uno"]));
    let (_, list) = call(&c.app, "GET", "/api/v0/packages", None).await;
    assert_eq!(list[0]["firmware"], json!(["uno"]));

    let package = json!({"kind": "package", "package": "my-bridge"});
    let (s, body) = flash(&c, UNO_PORT, json!({"firmware": package})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_on(&c.uno, &image);
    // It has nothing for a Mega: refused before the Mega's port is touched.
    let (s, e) = flash(&c, MEGA_PORT, json!({"firmware": package})).await;
    assert_eq!((s, e["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not-installed")), "{e}");
    assert!(c.mega.opens.lock().unwrap().is_empty());
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn next(ws: &mut Ws) -> Value {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
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
async fn the_flash_topic_follows_it_page_by_page() {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    let c = core();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = c.app.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/v0/ws?format=json")).await.unwrap();
    // Subscriptions are handled in order: once `ports` answers, `flash` is listening.
    for (id, kind) in [(1, "flash"), (2, "ports")] {
        let msg = json!({"type": "subscribe", "id": id, "topic": {"kind": kind}});
        ws.send(Message::Text(msg.to_string().into())).await.unwrap();
    }
    assert_eq!(next(&mut ws).await["type"], "ports");
    let path = put_hex(&c, "blink.hex", &program(300));
    let (s, body) = flash(&c, UNO_PORT, json!({"firmware": file(&path)})).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let mut seen = Vec::new();
    loop {
        let m = next(&mut ws).await;
        if m["type"] != "flash" {
            continue;
        }
        assert_eq!(
            (m["id"].as_u64(), m["port"].as_str(), m["total"].as_u64()),
            (Some(1), Some(UNO_PORT), Some(384))
        );
        seen.push(m["stage"].as_str().unwrap().to_string());
        if m["stage"] == "done" {
            break;
        }
    }
    assert_eq!(
        seen,
        ["connecting", "writing", "writing", "writing", "verifying", "verifying", "verifying", "done"]
    );
}
