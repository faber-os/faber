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
    core_on(connectors, replays, Arc::new(faber_core::ports::NoHardware))
}

/// The same, with these I²C, SPI and GPIO buses.
pub fn core_on(
    connectors: &[(&str, &str)],
    replays: Vec<(&str, Vec<Chunk>)>,
    hardware: Arc<dyn faber_core::ports::Hardware>,
) -> TestCore {
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
        hardware,
        replays: library,
        connectors: faber_core::connectors::built_in(),
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

/// The examples in examples/connectors, run through the engine on every build (docs/principles.md:
/// every example is tested). Their captures are written here, from known lines, and committed;
/// like the generated TypeScript types, commit what the tests write.
mod examples {
    use super::*;
    use faber_capabilities::{Reading, distance, position};
    use faber_connector::capture;
    use faber_connector::testing::RecordingContext;
    use faber_connector::{Health, Settings};
    use faber_core::declarative::{self, nmea};
    use std::path::{Path, PathBuf};

    fn folder(example: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/connectors").join(example)
    }

    fn faber_toml(example: &str) -> String {
        std::fs::read_to_string(folder(example).join("faber.toml")).unwrap()
    }

    /// Writes `chunks` as the example's capture if the file differs, and returns what the file
    /// holds, so the test checks the committed recording.
    fn capture_file(example: &str, chunks: &[Chunk]) -> Vec<Chunk> {
        let path = folder(example).join("captures").join(format!("{example}.fbcap"));
        let bytes = capture::encode(chunks);
        if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        capture::decode(&std::fs::read(&path).unwrap()).unwrap()
    }

    /// Plays a capture through the example's connector, with default settings.
    fn play(example: &str, chunks: &[Chunk]) -> RecordingContext {
        let def = declarative::definition(&faber_toml(example)).unwrap();
        assert_eq!(def.manifest.id, example, "the folder is named after the id");
        let mut c = (def.create)(&Settings::new()).unwrap();
        let mut ctx = RecordingContext::new();
        for chunk in chunks {
            c.on_bytes(&mut ctx, chunk.t_us, &chunk.bytes);
        }
        ctx
    }

    const CM: &[u32] = &[120, 110, 100, 90, 80, 70, 60, 50, 40, 30, 1, 30, 60, 90, 999, 120];

    /// The Arduino's lines, each split in two chunks 2 ms apart, 100 ms per line. Two are noise:
    /// a line cut off at power-up, and one damaged on the wire.
    fn distance_lines() -> (Vec<Chunk>, Vec<(u64, u32)>) {
        let mut lines: Vec<(String, Option<u32>)> = vec![("23\r\n".into(), None)];
        for (i, cm) in CM.iter().enumerate() {
            lines.push((format!("D:{cm}\r\n"), Some(*cm)));
            if i == 7 {
                lines.push(("D:4\u{7f}0\r\n".into(), None));
            }
        }
        let mut chunks = Vec::new();
        let mut expected = Vec::new();
        for (i, (line, cm)) in lines.iter().enumerate() {
            let t = 1_000_000 + i as u64 * 100_000;
            let (a, b) = line.as_bytes().split_at(2);
            chunks.push(Chunk { t_us: t, bytes: a.to_vec() });
            chunks.push(Chunk { t_us: t + 2_000, bytes: b.to_vec() });
            if let Some(cm) = cm {
                expected.push((t + 2_000, *cm));
            }
        }
        (chunks, expected)
    }

    #[test]
    fn line_distance_reads_the_arduinos_lines() {
        let (chunks, expected) = distance_lines();
        let ctx = play("line-distance", &capture_file("line-distance", &chunks));
        let info = distance::Info {
            min_m: 0.02,
            max_m: 4.0,
            field_of_view_rad: 0.26,
            kind: distance::Kind::Ultrasonic,
        };
        let want: Vec<(String, Reading)> = expected
            .iter()
            .map(|(t, cm)| {
                ("distance".to_string(), Reading::Distance(info.reading(*t, (f64::from(*cm) * 0.01) as f32)))
            })
            .collect();
        assert_eq!(ctx.published, want, "every good line, stamped when its last byte arrived");
        let statuses: Vec<_> = ctx
            .published
            .iter()
            .map(|(_, r)| match r {
                Reading::Distance(d) => d.status,
                _ => unreachable!(),
            })
            .collect();
        assert!(
            statuses.contains(&distance::Status::TooClose) && statuses.contains(&distance::Status::TooFar)
        );
        assert_eq!(ctx.health, [Health::Starting, Health::Running], "two noisy lines are not a problem");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn line_distance_runs_a_part_on_its_recording() {
        let (chunks, _) = distance_lines();
        let c = core(&[("line-distance", &faber_toml("line-distance"))], vec![("line-distance", chunks)]);
        let part = json!({"name": "front", "connector": "line-distance", "port": "replay:line-distance"});
        let (s, body) = call(&c.app, "POST", "/api/v0/parts", Some(part)).await;
        assert_eq!(s, StatusCode::CREATED, "{body}");
        assert_eq!(body["signals"][0]["info"]["info"]["maxM"], 4.0);
        let reading = first_reading(&c.app, "front/distance").await;
        assert_eq!(reading["capability"], "distance@1");
    }

    /// Minutes of longitude per second at 0.6 m/s eastwards, at 55.676° N.
    const LON_MIN_PER_S: f64 = 0.6 / (1852.0 * 0.563_6);

    /// Ten seconds from a NEO-6M: two without a fix, then walking east at about 0.6 m/s. Each
    /// second is one burst of RMC, VTG, GGA, GSA, GSV and GLL, sent in 32-byte chunks 33 ms apart
    /// (9600 baud). Returns the chunks and, per second, the time the GGA's last byte arrived.
    fn gps_seconds() -> (Vec<Chunk>, Vec<u64>) {
        let mut chunks = Vec::new();
        let mut gga_end = Vec::new();
        for s in 0..10u64 {
            let time = format!("1200{s:02}.00");
            let lon = format!("01234.{:05}", 10_000 + (s as f64 * LON_MIN_PER_S * 1e5).round() as u64);
            let bodies = if s < 2 {
                vec![
                    format!("GPRMC,{time},V,,,,,,,151026,,,N"),
                    "GPVTG,,,,,,,,,N".to_string(),
                    format!("GPGGA,{time},,,,,0,00,99.99,,,,,,"),
                    "GPGSA,A,1,,,,,,,,,,,,,99.99,99.99,99.99".to_string(),
                    "GPGSV,1,1,03,02,,,21,12,,,18,24,,,15".to_string(),
                    format!("GPGLL,,,,,{time},V,N"),
                ]
            } else {
                vec![
                    format!("GPRMC,{time},A,5540.56600,N,{lon},E,1.166,90.00,151026,,,A"),
                    "GPVTG,90.00,T,,M,1.166,N,2.160,K,A".to_string(),
                    format!("GPGGA,{time},5540.56600,N,{lon},E,1,08,1.01,14.2,M,41.5,M,,"),
                    "GPGSA,A,3,02,05,12,13,15,18,24,29,,,,,1.85,1.01,1.55".to_string(),
                    "GPGSV,2,1,08,02,40,083,46,05,25,140,38,12,63,280,44,13,30,050,41".to_string(),
                    format!("GPGLL,5540.56600,N,{lon},E,{time},A,A"),
                ]
            };
            let mut burst = String::new();
            for b in &bodies {
                burst.push_str(&nmea::sentence(b));
                burst.push_str("\r\n");
                if b.starts_with("GPGGA") {
                    let last_byte = burst.len() - 1;
                    gga_end.push(s * 1_000_000 + (last_byte / 32) as u64 * 33_000);
                }
            }
            for (k, piece) in burst.as_bytes().chunks(32).enumerate() {
                chunks.push(Chunk { t_us: s * 1_000_000 + k as u64 * 33_000, bytes: piece.to_vec() });
            }
        }
        (chunks, gga_end)
    }

    #[test]
    fn nmea_gps_reads_a_walk_across_the_square() {
        let (chunks, gga_end) = gps_seconds();
        let ctx = play("nmea-gps", &capture_file("nmea-gps", &chunks));
        let readings: Vec<&position::Reading> = ctx
            .published
            .iter()
            .map(|(name, r)| {
                assert_eq!(name, "position");
                match r {
                    Reading::Position(p) => p,
                    _ => unreachable!(),
                }
            })
            .collect();
        assert_eq!(readings.len(), 10, "one per GGA");
        let times: Vec<u64> = readings.iter().map(|r| r.t_us).collect();
        assert_eq!(times, gga_end, "stamped when the GGA's last byte arrived");
        for r in &readings[..2] {
            assert_eq!(**r, position::Reading::no_fix(r.t_us, 0));
        }
        for (i, r) in readings[2..].iter().enumerate() {
            assert_eq!((r.fix, r.satellites, r.altitude_m), (position::Fix::Fix3d, 8, Some(14.2)));
            assert!((r.latitude_deg - (55.0 + 40.566 / 60.0)).abs() < 1e-9);
            let lon = 12.0 + (34.1 + (i + 2) as f64 * LON_MIN_PER_S) / 60.0;
            assert!((r.longitude_deg - lon).abs() < 1e-6, "{} vs {lon}", r.longitude_deg);
            assert!((r.speed_m_s.unwrap() - 0.6).abs() < 0.001, "from the RMC of the same second");
            assert!((r.course_rad.unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        }
        let metres_per_s = (readings[9].longitude_deg - readings[2].longitude_deg) / 7.0 * 111_320.0 * 0.5636;
        assert!((metres_per_s - 0.6).abs() < 0.01, "{metres_per_s} m/s");
        assert_eq!(
            ctx.health,
            [Health::Starting, Health::Running],
            "running from the first good sentence, before a fix"
        );
    }
}

/// I²C register maps (docs/connectors-declarative.md): the TMP102 example on a fake bus.
mod i2c {
    use super::*;
    use faber_core::ports::fake::{FakeHardware, Tmp102};
    use std::path::Path;

    fn example() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/connectors/ti-tmp102/faber.toml");
        std::fs::read_to_string(path).unwrap()
    }

    /// The TMP102 manifest in docs/architecture.md §4, as written there.
    fn architecture_example() -> String {
        let doc =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/architecture.md"))
                .unwrap()
                .replace("\r\n", "\n");
        let from = doc.find("**Declarative example**").expect("the section is there");
        let block = &doc[from..];
        let start = block.find("```toml\n").unwrap() + "```toml\n".len();
        block[start..start + block[start..].find("```").unwrap()].to_string()
    }

    #[test]
    fn the_architecture_example_is_the_tested_example() {
        let doc = faber_core::declarative::manifest::parse(&architecture_example())
            .expect("it is a valid connector");
        let file = faber_core::declarative::manifest::parse(&example()).unwrap();
        assert_eq!(doc.manifest.port, file.manifest.port);
        assert_eq!(doc.rules, file.rules, "the same register read, the same scale");
    }

    #[tokio::test]
    async fn a_tmp102_on_the_bus_reads_the_right_celsius() {
        let celsius = Arc::new(std::sync::Mutex::new(21.5_f32));
        let knob = Arc::clone(&celsius);
        let hw = FakeHardware::new().with_i2c(1, 0x48, Tmp102::new(move || *knob.lock().unwrap()));
        let core = core_on(&[("ti-tmp102", &example())], vec![], Arc::new(hw));

        let (status, connectors) = call(&core.app, "GET", "/api/v0/connectors", None).await;
        assert_eq!(status, StatusCode::OK);
        let tmp = connectors.as_array().unwrap().iter().find(|c| c["id"] == "ti-tmp102").unwrap();
        assert_eq!(
            tmp["port"],
            json!({"kind": "i2c", "addresses": [0x48, 0x49, 0x4a, 0x4b], "default": 0x48})
        );

        let part = json!({"name": "cabin_temp", "connector": "ti-tmp102", "port": "i2c-1"});
        let (status, added) = call(&core.app, "POST", "/api/v0/parts", Some(part)).await;
        assert_eq!(status, StatusCode::CREATED, "{added}");
        assert_eq!(added["address"], 0x48, "the connector's default");
        let r = first_reading(&core.app, "cabin_temp/temperature").await;
        assert_eq!(r["capability"], "temperature@1");
        assert_eq!(r["reading"]["celsius"], 21.5);

        // −10.25 °C: a negative value keeps its sign through the shift.
        *celsius.lock().unwrap() = -10.25;
        let start = std::time::Instant::now();
        loop {
            let (_, v) = call(&core.app, "GET", "/api/v0/signals/cabin_temp/temperature", None).await;
            if v["reading"]["reading"]["celsius"] == -10.25 {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "{v}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let (_, ports) = call(&core.app, "GET", "/api/v0/ports", None).await;
        let bus = ports.as_array().unwrap().iter().find(|p| p["id"] == "i2c-1").unwrap();
        assert_eq!(bus["addresses"], json!([{"address": 0x48, "claimedBy": "cabin_temp"}]));
    }

    #[tokio::test]
    async fn a_sensor_that_does_not_answer_says_what_to_check() {
        let hw = FakeHardware::new().with_empty_i2c(1);
        let core = core_on(&[("ti-tmp102", &example())], vec![], Arc::new(hw));
        let part = json!({"name": "cabin_temp", "connector": "ti-tmp102", "port": "i2c-1", "address": 0x49});
        let (status, added) = call(&core.app, "POST", "/api/v0/parts", Some(part)).await;
        assert_eq!(status, StatusCode::CREATED, "{added}");
        let start = std::time::Instant::now();
        loop {
            let (_, parts) = call(&core.app, "GET", "/api/v0/parts", None).await;
            let state = &parts[0]["state"];
            if state["kind"] == "failed" {
                let message = state["message"].as_str().unwrap();
                assert!(message.contains("didn't answer") && message.contains("0x49"), "{state}");
                assert!(state["hint"].as_str().unwrap().contains("I2C scanner"), "{state}");
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(8), "{state}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}
