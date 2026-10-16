//! The generator's tests: golden files for every template on every board (`fixtures/wiring/`),
//! and the same sketches compiled with the real `arduino-cli` when this computer has one.
//!
//! `FABER_REFRESH_GOLDEN=1 cargo test -p faber-core golden` rewrites the golden files after a
//! deliberate change; review the diff before committing it. CI's Arduino job compiles them too.

use super::*;
use crate::arduino::wiring::allocator::{Candidate, suggest};
use crate::arduino::wiring::parts::{BuiltIn, templates};
use crate::arduino::wiring::pins;
use crate::arduino::wiring::plan;
use std::path::{Path, PathBuf};

/// A plan for `board` with one part of every template (every shape that can be used), each on the
/// pins the allocator suggests: what the golden files and the compile test cover.
fn everything(board: &str) -> Plan {
    let map = pins::arduino(board).unwrap();
    let mut p = Plan::new(board, board);
    for t in templates() {
        for shape in &t.shapes {
            let c = Candidate { template: t.id.clone(), shape: Some(shape.id.clone()), ..Default::default() };
            let s = suggest(&map, &p, &BuiltIn, &[], &c).unwrap();
            assert!(s.roles.iter().all(|r| r.suggested.is_some()), "{board}: no room for {}", t.id);
            p.parts.insert(s.name, s.part);
        }
    }
    p
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/wiring")
}

/// The files of a guided board's folder, as Faber writes them.
fn folder(board: &str) -> Vec<(String, String)> {
    let map = pins::arduino(board).unwrap();
    let p = everything(board);
    vec![
        (plan::FILE.to_string(), plan::write(None, &p, &map, &BuiltIn)),
        (format!("{board}.ino"), sketch(&p, &map, &BuiltIn).unwrap()),
        (MY_CODE.to_string(), my_code(&p)),
    ]
}

#[test]
fn golden_sketches_for_every_template_on_every_board() {
    let refresh = std::env::var_os("FABER_REFRESH_GOLDEN").is_some();
    for board in pins::arduino_boards() {
        let dir = golden_dir().join(board);
        for (name, text) in folder(board) {
            let path = dir.join(&name);
            if refresh {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &text).unwrap();
                continue;
            }
            let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "{} is missing: run FABER_REFRESH_GOLDEN=1 cargo test -p faber-core golden",
                    path.display()
                )
            });
            // Git may check text out with CRLF on Windows.
            assert_eq!(
                text,
                golden.replace("\r\n", "\n"),
                "{} changed: if that is deliberate, run FABER_REFRESH_GOLDEN=1 cargo test -p faber-core golden and review the diff",
                path.display()
            );
        }
        // The plan read back from its file gives the same sketch.
        let map = pins::arduino(board).unwrap();
        let files = folder(board);
        let read = plan::parse(&files[0].1).unwrap().normalized(&map, &BuiltIn);
        assert_eq!(sketch(&read, &map, &BuiltIn).unwrap(), files[1].1, "{board}");
    }
}

#[test]
fn the_same_plan_gives_the_same_bytes() {
    let a = folder("mega2560");
    let b = folder("mega2560");
    assert_eq!(a, b);
}

#[test]
fn a_sketch_has_the_mark_the_parts_setup_and_loop() {
    let map = pins::arduino("uno").unwrap();
    let mut p = Plan::new("uno", "Rover");
    p.parts.insert(
        "front".into(),
        PlannedPart {
            template: "ultrasonic".into(),
            pins: [("trig".to_string(), "7".to_string()), ("echo".to_string(), "6".to_string())].into(),
            ..Default::default()
        },
    );
    let p = p.normalized(&map, &BuiltIn);
    let s = sketch(&p, &map, &BuiltIn).unwrap();
    assert!(is_generated(&s));
    assert!(s.starts_with("// Rover.ino: Faber writes this file from board.toml."), "{s}");
    assert!(s.contains("//   front: VCC → 5V, GND → GND, Trig → D7, Echo → D6\n"), "{s}");
    assert!(s.contains("#include <FaberLink.h>\n\nFaberLink link(Serial);\n"), "{s}");
    assert!(s.contains("// front: Ultrasonic distance sensor, Trig D7, Echo D6\nFaberUltrasonic front(\"front\", 7, 6, 0.02, 4.0);\n"), "{s}");
    assert!(s.contains("#include \"my_code.h\""));
    assert!(s.contains("  link.begin(115200, \"rover\", \"guided\");\n  userSetup();\n"), "{s}");
    assert!(s.ends_with("void loop() {\n  link.poll();\n  userLoop();\n}\n"), "{s}");
    let mine = my_code(&p);
    assert!(mine.contains("void userSetup() {\n}") && mine.contains("void userLoop() {\n}"));
    assert!(mine.contains("no delay()"));
    assert!(!is_generated(&mine));
}

#[test]
fn the_arguments_come_from_pins_settings_and_the_board() {
    let map = pins::arduino("mega2560").unwrap();
    let mut p = Plan::new("mega2560", "mega");
    let mut battery = PlannedPart { template: "battery".into(), ..Default::default() };
    battery.pins.insert("sense".into(), "A1".into());
    battery.settings.insert("chemistry".into(), SettingValue::Text("lipo".into()));
    battery.settings.insert("capacity-ah".into(), SettingValue::Number(15.0));
    p.parts.insert("battery".into(), battery);
    let mut side = PlannedPart { template: "ir-distance".into(), ..Default::default() };
    side.pins.insert("out".into(), "A0".into());
    p.parts.insert("side".into(), side);
    let s = sketch(&p.normalized(&map, &BuiltIn), &map, &BuiltIn).unwrap();
    assert!(
        s.contains("FaberBattery battery(\"battery\", A1, 3.0, 3, faberlink::BATTERY_LIPO, 5.0, 15.0);"),
        "{s}"
    );
    assert!(s.contains("float sideToMeters(int raw) {\n  float volts = raw * 5.0f / 1023.0f;"), "{s}");
    assert!(
        s.contains("FaberDistance side(\"side\", A0, 0.1, 0.8, faberlink::DISTANCE_INFRARED, sideToMeters);"),
        "{s}"
    );
    assert!(
        s.contains("//   battery: Battery + → R1 (20000 Ω) → A1, A1 → R2 (10000 Ω) → GND, Battery - → GND\n"),
        "{s}"
    );
}

#[test]
fn a_plan_with_errors_gives_no_sketch() {
    let map = pins::arduino("uno").unwrap();
    let mut p = Plan::new("uno", "x");
    p.parts.insert(
        "front".into(),
        PlannedPart {
            template: "ultrasonic".into(),
            pins: [("trig".to_string(), "D0".to_string())].into(),
            ..Default::default()
        },
    );
    let errors = sketch(&p, &map, &BuiltIn).unwrap_err();
    assert_eq!(errors.iter().map(|e| e.code.as_str()).collect::<Vec<_>>(), ["reserved-pin", "missing-pin"]);
}

#[test]
fn taking_over_keeps_the_code_and_drops_the_mark() {
    let map = pins::arduino("uno").unwrap();
    let files = folder("uno");
    let ino = &files[1].1;
    let mine = taken_over(ino, "uno", "16 Oct 2026");
    assert!(!is_generated(&mine));
    assert!(
        mine.starts_with(
            "// uno.ino: your code. Faber wrote it from board.toml until you took it over (16 Oct 2026);"
        ),
        "{mine}"
    );
    assert!(mine.contains("// Board: Arduino Uno (uno)\n"), "the wiring note stays: {mine}");
    let body = &ino[ino.find("\n\n").unwrap() + 1..];
    assert!(mine.ends_with(body), "everything after the header is unchanged");
    let _ = map;
}

#[test]
fn wiring_lines_fill_in_pins_and_settings() {
    let map = pins::arduino("uno").unwrap();
    let p = everything("uno");
    let w = wiring(&p, &map, &BuiltIn);
    let front = w.iter().find(|w| w.template == "ultrasonic").unwrap();
    assert_eq!(front.lines, ["VCC → 5V", "GND → GND", "Trig → D4", "Echo → D7"]);
    assert!(w.iter().find(|w| w.template == "motor").unwrap().warnings[0].contains("own supply"));
}

/// Compiles every golden sketch with the real `arduino-cli` and `--warnings all`, when this
/// computer has one (as the editor finds it: on the PATH or inside the Arduino IDE 2) with the
/// AVR core. Skipped otherwise; CI's Arduino job compiles the same files.
#[test]
fn every_generated_sketch_compiles_with_the_real_arduino_cli() {
    use crate::arduino::toolchain::{Places, locate};
    let tmp = tempfile::tempdir().unwrap();
    let Some((exe, _)) = locate(&Places::of_this_computer(tmp.path())) else {
        eprintln!("skipped: no arduino-cli on this computer");
        return;
    };
    if std::env::var_os("FABER_SKIP_ARDUINO_CLI").is_some() {
        eprintln!("skipped: FABER_SKIP_ARDUINO_CLI is set");
        return;
    }
    let library = crate::arduino::library::install(tmp.path()).unwrap();
    for board in pins::arduino_boards() {
        let fqbn = crate::arduino::fqbn(board).unwrap();
        let dir = tmp.path().join("sketches").join(board);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, text) in folder(board) {
            std::fs::write(dir.join(name), text).unwrap();
        }
        // Kept between runs, so only the first run compiles the Arduino core.
        let build = std::env::temp_dir().join("faber-wiring-compile").join(board);
        let out = std::process::Command::new(&exe)
            .args(["compile", "--fqbn", fqbn, "--warnings", "all", "--format", "json", "--build-path"])
            .arg(&build)
            .arg("--library")
            .arg(&library)
            .arg(&dir)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{board}: arduino-cli answered something unreadable ({e}): {}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
        if json
            .get("error")
            .and_then(|e| e.as_str())
            .is_some_and(|e| e.contains("not found") || e.contains("not installed"))
        {
            eprintln!("skipped: {}", json["error"]);
            return;
        }
        let compiler = format!(
            "{}{}",
            json["compiler_out"].as_str().unwrap_or(""),
            json["compiler_err"].as_str().unwrap_or("")
        );
        assert_eq!(json["success"], true, "{board} doesn't compile:\n{compiler}\n{}", json["error"]);
        let sketch_dir = dir.to_string_lossy().replace('\\', "/");
        let warnings: Vec<&str> = compiler
            .lines()
            .filter(|l| {
                let l = l.replace('\\', "/");
                l.contains("warning:")
                    && (l.contains(&sketch_dir)
                        || l.contains("FaberLink")
                        || l.contains(".ino:")
                        || l.contains(MY_CODE))
            })
            .collect();
        assert!(
            warnings.is_empty(),
            "{board}: warnings in the generated sketch or the library:\n{}",
            warnings.join("\n")
        );
        eprintln!(
            "{board}: compiled ({})",
            compiler.lines().find(|l| l.starts_with("Sketch uses")).unwrap_or("size unknown")
        );
    }
}
