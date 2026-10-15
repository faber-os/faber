//! `faber check`: builds the connector (wasm: for `wasm32-unknown-unknown`, then wrapped as a
//! component), loads it the way the core does, and replays every recording in `captures/`
//! through it, reporting the readings and the health it ends with.
//!
//! For an app: reads its faber.toml with the core's reader, checks its UI, builds its logic (if
//! it has any), loads it in the core's sandbox, and runs it for a moment with no parts picked
//! for its roles, so a crash at start shows up here and not on the machine.

use crate::CliError;
use faber_capabilities::Reading;
use faber_connector::capture::{self, Chunk};
use faber_connector::manifest_file::APP_TYPE;
use faber_connector::manifest_file::{self, PackageHeader};
use faber_connector::{Context, Definition, Health, Problem, Settings};
use faber_core::{apps, declarative, packages, wasm};
use std::collections::BTreeMap;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TARGET: &str = "wasm32-unknown-unknown";

/// Files a package must hold besides the connector itself.
pub const REQUIRED: [&str; 3] = ["faber.toml", "README.md", "LICENSE"];

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Where cargo builds the wasm module. Default: the project's `target/` (or
    /// `CARGO_TARGET_DIR`).
    pub target_dir: Option<PathBuf>,
}

/// What one recording did to the connector.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureResult {
    /// The file's name in `captures/`.
    pub name: String,
    pub chunks: usize,
    /// Readings per signal.
    pub readings: BTreeMap<String, usize>,
    /// The last reading, in words (`1.230 m, ok`).
    pub last: Option<String>,
    /// The health it ended with.
    pub health: Option<Health>,
    /// Set when it crashed (or broke a sandbox rule) on this recording.
    pub crash: Option<Problem>,
}

impl CaptureResult {
    /// It gave readings, didn't crash and didn't end failed.
    pub fn passed(&self) -> bool {
        self.crash.is_none() && !self.readings.is_empty() && !matches!(self.health, Some(Health::Failed(_)))
    }
}

/// Everything `faber check` found.
#[derive(Clone, Debug)]
pub struct Report {
    pub header: PackageHeader,
    /// The component, for a wasm connector, ready to pack.
    pub component: Option<Vec<u8>>,
    /// The component's file name in the package (`connector.wasm` unless faber.toml says).
    pub module: Option<String>,
    pub captures: Vec<CaptureResult>,
    /// An app's UI files, by their path in the package (`ui/index.html`, …). Empty for a
    /// connector.
    pub ui: Vec<String>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.captures.iter().all(CaptureResult::passed)
    }

    /// The report as the command prints it.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.header.kind == APP_TYPE {
            return out; // its checks print as they go, and fail with an error
        }
        if self.captures.is_empty() {
            out.push(
                "No recordings in captures/. Record your device in Faber's serial monitor and put the .fbcap file there, so faber check can test the parser.".into(),
            );
        }
        for c in &self.captures {
            let readings = if c.readings.is_empty() {
                "no readings".to_string()
            } else {
                c.readings.iter().map(|(s, n)| format!("{n} on {s}")).collect::<Vec<_>>().join(", ")
            };
            let health = match &c.health {
                None => "no health reported".to_string(),
                Some(h) => format!("health {}", health_words(h)),
            };
            let mark = if c.passed() { "ok  " } else { "FAIL" };
            out.push(format!("{mark} captures/{}: {} chunks, {readings}; {health}", c.name, c.chunks));
            if let Some(last) = &c.last {
                out.push(format!("       last reading: {last}"));
            }
            if let Some(p) = &c.crash {
                out.push(format!("       it crashed: {}", p.message));
                out.push(format!("       {}", p.hint));
            } else if c.readings.is_empty() {
                out.push("       The parser recognised nothing in this recording. Check that it understands what the device sends (look at the recording in Faber's serial monitor).".into());
            }
        }
        out
    }
}

fn health_words(h: &Health) -> String {
    match h {
        Health::Starting => "starting (never got good data)".into(),
        Health::Running => "running".into(),
        Health::Degraded(p) => format!("degraded: {} {}", p.message, p.hint),
        Health::Failed(p) => format!("failed: {} {}", p.message, p.hint),
    }
}

/// Checks the connector project in `dir`. `out` gets a line per step. Fails at the first thing
/// that stops the connector from being packed at all; how each recording went is in the report.
pub fn check(dir: &Path, opts: &Options, out: &mut dyn Write) -> Result<Report, CliError> {
    let read = |name: &str| {
        std::fs::read_to_string(dir.join(name)).map_err(|e| {
            CliError::new(
                format!("{} has no readable {name} ({e}).", dir.display()),
                "Run faber check in a connector's folder (the one with faber.toml), or start one with faber new connector <name>.",
            )
        })
    };
    let toml = read("faber.toml")?;
    let header = manifest_file::header(&toml)
        .map_err(|p| CliError::new(format!("faber.toml: {}", p.message), p.hint))?;
    if header.kind == APP_TYPE {
        let _ = writeln!(out, "Checking {} {} (app)", header.id, header.version);
        return check_app(dir, &toml, header, opts, out);
    }
    let _ = writeln!(out, "Checking {} {} ({} connector)", header.id, header.version, header.kind);
    for file in REQUIRED {
        if !dir.join(file).is_file() {
            return Err(CliError::new(
                format!("The connector has no {file}."),
                "A package needs faber.toml, README.md (shown before installing) and LICENSE. Add the missing file.",
            ));
        }
    }
    let (definition, component, module) = match header.kind.as_str() {
        "wasm" => {
            let package = manifest_file::parse(&toml)
                .map_err(|p| CliError::new(format!("faber.toml: {}", p.message), p.hint))?;
            let component = build_component(dir, opts, out)?;
            let def = wasm::definition(package.manifest, &component, wasm::Limits::default())?;
            let _ = writeln!(
                out,
                "ok   The component ({} KB) loads in the sandbox and asks only for the connector world.",
                component.len().div_ceil(1024)
            );
            (def, Some(component), package.module)
        }
        "declarative" => {
            let def = declarative::definition(&toml).map_err(|e| {
                let place = e.line.map(|l| format!(" line {l}")).unwrap_or_default();
                CliError::new(format!("faber.toml{place}: {}", e.message), e.hint)
            })?;
            let _ = writeln!(out, "ok   faber.toml: every rule reads.");
            (def, None, None)
        }
        other => {
            return Err(CliError::new(
                format!("faber.toml says kind = \"{other}\", which faber check can't check."),
                "Use kind = \"wasm\" (Rust) or kind = \"declarative\" (only faber.toml).",
            ));
        }
    };
    let mut captures = Vec::new();
    for (name, chunks) in recordings(dir)? {
        captures.push(replay(&definition, name, &chunks));
    }
    Ok(Report { header, component, module, captures, ui: Vec::new() })
}

/// Plays the machine for an app under check: every lease and command is refused (no parts).
struct NoParts;

impl apps::Driver for NoParts {
    fn acquire(&self, _: &faber_core::names::PartName, _: &str) -> Result<String, apps::DriveError> {
        Err(apps::DriveError::NoSuchRole)
    }
    fn renew(&self, _: &faber_core::names::PartName, _: &str) -> Result<(), apps::DriveError> {
        Err(apps::DriveError::NoSuchRole)
    }
    fn release(&self, _: &faber_core::names::PartName, _: &str) {}
    fn command(
        &self,
        _: &faber_core::names::PartName,
        _: &str,
        _: faber_capabilities::Command,
    ) -> Result<faber_capabilities::Command, apps::DriveError> {
        Err(apps::DriveError::NoSuchRole)
    }
}

/// How many ticks `faber check` runs an app for.
const SMOKE_TICKS: u64 = 20;

fn check_app(
    dir: &Path,
    toml: &str,
    header: PackageHeader,
    opts: &Options,
    out: &mut dyn Write,
) -> Result<Report, CliError> {
    for file in REQUIRED {
        if !dir.join(file).is_file() {
            return Err(CliError::new(
                format!("The app has no {file}."),
                "A package needs faber.toml, README.md (shown before installing) and LICENSE. Add the missing file.",
            ));
        }
    }
    let manifest = apps::manifest::parse(toml).map_err(|e| {
        let place = e.line.map(|l| format!(" line {l}")).unwrap_or_default();
        CliError::new(format!("faber.toml{place}: {}", e.message), e.hint)
    })?;
    let _ = writeln!(
        out,
        "ok   faber.toml: {} roles, {} derived signals.",
        manifest.requires.len(),
        manifest.publishes.len()
    );
    let ui = if manifest.ui { ui_files(dir)? } else { Vec::new() };
    if manifest.ui {
        let _ = writeln!(out, "ok   ui/: {} files, starting at ui/index.html.", ui.len());
    }
    let (component, module) = match &manifest.module {
        None => (None, None),
        Some(module) => {
            let component = build_component(dir, opts, out)?;
            let compiled = apps::compile(&manifest.name, &component, faber_core::sandbox::Limits::default())?;
            let _ = writeln!(
                out,
                "ok   The component ({} KB) loads in the sandbox and asks only for the app world.",
                component.len().div_ceil(1024)
            );
            smoke_run(&manifest, &compiled)?;
            let _ = writeln!(
                out,
                "ok   It starts, runs {SMOKE_TICKS} ticks and stops, with no parts picked for its roles."
            );
            (Some(component), Some(module.clone()))
        }
    };
    Ok(Report { header, component, module, captures: Vec::new(), ui })
}

/// Runs the app's logic for a moment with its roles empty, as on a machine before its parts
/// are picked.
fn smoke_run(manifest: &apps::manifest::AppManifest, compiled: &apps::Compiled) -> Result<(), CliError> {
    let roles = manifest
        .requires
        .iter()
        .map(|spec| apps::BoundRole { spec: spec.clone(), parts: Vec::new() })
        .collect();
    let session = apps::Session::new(
        &manifest.id,
        roles,
        manifest.publishes.clone(),
        std::sync::Arc::new(faber_core::router::Router::new()),
        std::sync::Arc::new(NoParts),
        std::sync::Arc::new(apps::SharedState::in_memory(&manifest.id)),
    );
    let crashed = |p: Problem| {
        CliError::new(
            format!("The app crashed with no parts picked: {}", p.message),
            "An app starts before its parts are picked, and parts can go away: handle a role without a reading.",
        )
    };
    let mut logic = compiled.instantiate(session).map_err(crashed)?;
    use apps::Logic;
    logic.start(0).map_err(crashed)?;
    for i in 1..=SMOKE_TICKS {
        logic.tick(i * 1_000_000 / u64::from(manifest.tick_hz)).map_err(crashed)?;
    }
    logic.stop().map_err(crashed)
}

/// Every file in `ui/`, as `ui/<path>`, checked like the installer checks them.
fn ui_files(dir: &Path) -> Result<Vec<String>, CliError> {
    fn walk(root: &Path, rel: &str, out: &mut Vec<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(root.join(rel))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            if entry.file_type()?.is_dir() {
                walk(root, &path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(&dir.join("ui"), "", &mut files).map_err(|e| {
        CliError::new(
            format!("The app's ui/ folder can't be read ({e})."),
            "Add ui/index.html, or remove ui = \"ui/\" from faber.toml.",
        )
    })?;
    files.sort();
    if !files.iter().any(|f| f == "index.html") {
        return Err(CliError::new(
            "The app has no ui/index.html.",
            "An app's UI starts at ui/index.html. Add it.",
        ));
    }
    if let Some(bad) = files.iter().find(|f| !packages::ui_path_ok(f)) {
        return Err(CliError::new(
            format!("ui/{bad} can't go in the package."),
            "UI files have plain names (letters, digits, ., - and _, none starting with .) and a web file type (html, css, js, svg, png, …).",
        ));
    }
    Ok(files.into_iter().map(|f| format!("ui/{f}")).collect())
}

/// Every `captures/*.fbcap`, by name.
fn recordings(dir: &Path) -> Result<Vec<(String, Vec<Chunk>)>, CliError> {
    let Ok(entries) = std::fs::read_dir(dir.join("captures")) else { return Ok(Vec::new()) };
    let mut paths: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "fbcap")).collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let bytes = std::fs::read(&p).map_err(|e| {
                CliError::new(format!("captures/{name} can't be read: {e}"), "Check the file.")
            })?;
            let chunks = capture::decode(&bytes).map_err(|_| {
                CliError::new(
                    format!("captures/{name} isn't a Faber recording (.fbcap)."),
                    "Record with Faber's serial monitor, or remove the file from captures/.",
                )
            })?;
            Ok((name, chunks))
        })
        .collect()
}

/// Remembers what a connector did, like the core's part context.
#[derive(Default)]
struct Recorder {
    readings: BTreeMap<String, usize>,
    last: Option<Reading>,
    health: Option<Health>,
}

impl Context for Recorder {
    fn publish(&mut self, name: &str, reading: Reading) {
        *self.readings.entry(name.to_string()).or_default() += 1;
        self.last = Some(reading);
    }

    fn set_health(&mut self, health: Health) {
        self.health = Some(health);
    }
}

/// Feeds a recording to a fresh connector the way the core does: bytes as they arrived, and a
/// tick every 100 ms of recorded time.
pub fn replay(definition: &Definition, name: String, chunks: &[Chunk]) -> CaptureResult {
    let mut ctx = Recorder::default();
    let crash = match (definition.create)(&Settings::new()) {
        Err(e) => Some(Problem::new(format!("Setting \"{}\": {}", e.key, e.message), e.hint)),
        Ok(mut connector) => {
            let run = catch_unwind(AssertUnwindSafe(|| {
                let mut next_tick = chunks.first().map(|c| c.t_us).unwrap_or(0);
                for chunk in chunks {
                    while next_tick <= chunk.t_us {
                        connector.on_tick(&mut ctx, next_tick);
                        next_tick += 100_000;
                    }
                    connector.on_bytes(&mut ctx, chunk.t_us, &chunk.bytes);
                }
            }));
            run.err().map(|payload| match payload.downcast::<Problem>() {
                Ok(p) => *p,
                Err(other) => {
                    let text = other
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| other.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown".into());
                    Problem::new(
                        format!("It panicked: {text}"),
                        "Fix the code that panicked; the message says where.",
                    )
                }
            })
        }
    };
    CaptureResult {
        name,
        chunks: chunks.len(),
        readings: ctx.readings,
        last: ctx.last.as_ref().map(describe),
        health: ctx.health,
        crash,
    }
}

/// `TooFar` → `too-far`, as the API writes it.
fn words(value: impl std::fmt::Debug) -> String {
    let mut out = String::new();
    for (i, c) in format!("{value:?}").chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// A reading in a few words.
fn describe(r: &Reading) -> String {
    match r {
        Reading::Distance(d) => format!("{:.3} m, {}", d.meters, words(d.status)),
        Reading::Position(p) => format!(
            "{:.6}, {:.6}, fix {}, {} satellites",
            p.latitude_deg,
            p.longitude_deg,
            words(p.fix),
            p.satellites
        ),
        Reading::LaserScan(s) => format!("a scan of {} points", s.ranges.len()),
        other => other.capability().id().to_string(),
    }
}

/// Builds the project's library for wasm and wraps it as a component.
fn build_component(dir: &Path, opts: &Options, out: &mut dyn Write) -> Result<Vec<u8>, CliError> {
    if !have_target() {
        return Err(CliError::new(
            format!("Rust can't build for {TARGET} on this computer yet."),
            format!("Run: rustup target add {TARGET}"),
        ));
    }
    let cargo_toml = std::fs::read_to_string(dir.join("Cargo.toml")).map_err(|e| {
        CliError::new(
            format!("A wasm connector is a Rust project, but {} has no readable Cargo.toml ({e}).", dir.display()),
            "Start one with faber new connector <name>, or use kind = \"declarative\" for a connector that is only faber.toml.",
        )
    })?;
    let crate_name = cargo_toml
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("package")?.get("name")?.as_str().map(|n| n.replace('-', "_")))
        .ok_or_else(|| {
            CliError::new("Cargo.toml has no [package] name.", "Add name = \"my-sensor\" under [package].")
        })?;
    let target_dir = match &opts.target_dir {
        Some(t) => t.clone(),
        None => {
            std::env::var_os("CARGO_TARGET_DIR").map(|t| dir.join(t)).unwrap_or_else(|| dir.join("target"))
        }
    };
    let _ = writeln!(out, "Building for {TARGET} …");
    let _ = out.flush();
    let status = Command::new(cargo())
        .current_dir(dir)
        .args(["rustc", "--lib", "--release", "--target", TARGET, "--crate-type", "cdylib", "--target-dir"])
        .arg(&target_dir)
        .status()
        .map_err(|e| {
            CliError::new(format!("cargo couldn't be started: {e}"), "Install Rust from https://rustup.rs.")
        })?;
    if !status.success() {
        return Err(CliError::new(
            "The connector didn't build.",
            "Fix the errors cargo printed above, then run faber check again.",
        ));
    }
    let file = target_dir.join(TARGET).join("release").join(format!("{crate_name}.wasm"));
    let module = std::fs::read(&file).map_err(|e| {
        CliError::new(
            format!("The build left no {} ({e}).", file.display()),
            "Run faber check again; if it keeps happening, report it.",
        )
    })?;
    wit_component::ComponentEncoder::default()
        .module(&module)
        .and_then(|e| e.validate(true).encode())
        .map_err(|e| {
            CliError::new(
                format!("The built module can't be made into a component: {e}"),
                "Check that src/lib.rs exports it for wasm32, as the template does: faber_connector_wasm::export!(definition); for a connector, faber_app_wasm::export!(MyApp); for an app.",
            )
        })
}

fn cargo() -> PathBuf {
    std::env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| "cargo".into())
}

/// Whether the wasm target is installed for the toolchain `cargo` uses.
pub fn have_target() -> bool {
    let rustc = cargo().with_file_name(format!("rustc{}", std::env::consts::EXE_SUFFIX));
    let rustc = if rustc.exists() { rustc } else { PathBuf::from("rustc") };
    let Ok(out) = Command::new(rustc).args(["--print", "target-libdir", "--target", TARGET]).output() else {
        return false;
    };
    let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    out.status.success() && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some())
}
