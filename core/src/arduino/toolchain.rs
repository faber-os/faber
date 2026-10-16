//! Compiling a sketch with `arduino-cli`, and finding one on the computer.
//!
//! The core uses `arduino-cli` only through [`Toolchain`], so every test runs with a fake
//! ([`super::fake`]) and no test ever starts the real program. [`ArduinoCli`] is the real one:
//! it runs the program with its arguments (never through a shell) and reads its JSON output.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// What compiling needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileJob {
    /// The sketch folder (`…/Blink/` with `Blink.ino`).
    pub sketch_dir: PathBuf,
    pub fqbn: String,
    /// Kept between compiles, so the second one is fast.
    pub build_dir: PathBuf,
    /// Where the `.hex` goes.
    pub output_dir: PathBuf,
    /// Library folders passed with `--library` (the FaberLink library).
    pub libraries: Vec<PathBuf>,
    /// Compiled in as `FABER_BUILD_ID` (docs/faberlink.md, `HELLO_ACK`).
    pub build_id: u64,
}

/// How much of the board the program takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Program (flash) bytes, and how many the board has room for.
    pub flash: u32,
    pub flash_max: u32,
    /// Global variables (RAM) at start, and the board's RAM.
    pub ram: u32,
    pub ram_max: u32,
}

/// What a compile gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    pub success: bool,
    /// The Intel HEX text, when it worked.
    pub hex: Option<String>,
    /// The compiler's own output (errors and warnings), for [`super::diagnostics::parse`].
    pub output: String,
    pub size: Option<Size>,
    /// What `arduino-cli` itself said went wrong, when it isn't a compiler error (no such board,
    /// a missing core …).
    pub error: Option<String>,
}

/// The program could not be run, or said something Faber can't read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError(pub String);

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where an `arduino-cli` was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Installed by Faber into `<data>/tools/arduino-cli/`.
    Faber,
    /// On the `PATH`.
    Path,
    /// The copy the Arduino IDE 2 brings with it.
    ArduinoIde,
    /// A stand-in (tests, and the core's `--fake-arduino-cli`).
    Fake,
}

/// Which `arduino-cli` it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainInfo {
    pub path: PathBuf,
    pub source: Source,
    pub version: Option<String>,
}

pub trait Toolchain: Send + Sync {
    fn info(&self) -> ToolchainInfo;
    /// Whether the Arduino AVR core (the compiler and the boards' files) is installed.
    fn has_avr_core(&self) -> Result<bool, ToolError>;
    /// Installs the AVR core (`arduino:avr`), about 150 MB from Arduino's servers. Each line of
    /// the program's output goes to `line`.
    fn install_avr_core(&self, line: &mut dyn FnMut(&str)) -> Result<(), ToolError>;
    fn compile(&self, job: &CompileJob) -> Result<Compiled, ToolError>;
}

/// Finds the toolchain to use now: it may have been installed, or removed, since the last time.
pub trait Finder: Send + Sync {
    fn find(&self) -> Option<Arc<dyn Toolchain>>;
}

/// The program's file name on this OS.
pub fn exe_name() -> &'static str {
    if cfg!(windows) { "arduino-cli.exe" } else { "arduino-cli" }
}

/// Where Faber installs its own copy: `<data>/tools/arduino-cli/`.
pub fn faber_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tools").join("arduino-cli")
}

/// The places to look, from the environment, so tests can give their own.
#[derive(Clone, Debug, Default)]
pub struct Places {
    pub data_dir: PathBuf,
    /// `PATH`.
    pub path: Option<OsString>,
    /// Windows: `%LOCALAPPDATA%` (a per-user install of the IDE) and `%ProgramFiles%`.
    pub local_app_data: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
    /// macOS: the IDE in `/Applications`.
    pub applications: Option<PathBuf>,
}

impl Places {
    pub fn of_this_computer(data_dir: &Path) -> Places {
        Places {
            data_dir: data_dir.to_path_buf(),
            path: std::env::var_os("PATH"),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            program_files: std::env::var_os("ProgramFiles").map(PathBuf::from),
            applications: cfg!(target_os = "macos").then(|| PathBuf::from("/Applications")),
        }
    }
}

/// The Arduino IDE 2 keeps its own `arduino-cli` here, under its install folder.
const IDE_BACKEND: &[&str] = &["resources", "app", "lib", "backend", "resources"];

/// Finds an `arduino-cli`: Faber's own copy first (it was installed for Faber, with its own
/// data folder), then one on the `PATH`, then the one inside the Arduino IDE 2.
pub fn locate(places: &Places) -> Option<(PathBuf, Source)> {
    let exe = exe_name();
    let own = faber_dir(&places.data_dir).join(exe);
    if own.is_file() {
        return Some((own, Source::Faber));
    }
    if let Some(path) = &places.path {
        for dir in std::env::split_paths(path) {
            let p = dir.join(exe);
            if p.is_file() {
                return Some((p, Source::Path));
            }
        }
    }
    let mut ide = Vec::new();
    if let Some(l) = &places.local_app_data {
        ide.push(l.join("Programs").join("Arduino IDE"));
    }
    if let Some(p) = &places.program_files {
        ide.push(p.join("Arduino IDE"));
    }
    if let Some(a) = &places.applications {
        ide.push(a.join("Arduino IDE.app").join("Contents"));
    }
    ide.into_iter()
        .map(|root| IDE_BACKEND.iter().fold(root, |p, s| p.join(s)).join(exe))
        .find(|p| p.is_file())
        .map(|p| (p, Source::ArduinoIde))
}

/// The real program.
pub struct ArduinoCli {
    pub exe: PathBuf,
    pub source: Source,
    /// Faber's own copy uses its own config (its own data folder under `<data>/tools/`); one that
    /// was already on the computer uses its owner's, so the boards they installed are found.
    pub config: Option<PathBuf>,
}

impl ArduinoCli {
    fn command(&self) -> Command {
        let mut c = Command::new(&self.exe);
        if let Some(cfg) = &self.config {
            c.arg("--config-file").arg(cfg);
        }
        c.stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        c
    }

    fn run_json(&self, args: &[&str]) -> Result<serde_json::Value, ToolError> {
        let out =
            self.command().args(args).args(["--format", "json"]).output().map_err(|e| self.cant_run(e))?;
        serde_json::from_slice(&out.stdout).map_err(|e| {
            ToolError(format!(
                "arduino-cli {} answered something Faber can't read ({e}): {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        })
    }

    fn cant_run(&self, e: io::Error) -> ToolError {
        ToolError(format!("Faber could not start {}: {e}", self.exe.display()))
    }
}

/// The `executable_sections_size` of `arduino-cli compile --format json`: `text` is flash,
/// `data` is RAM.
fn size_of(result: &serde_json::Value) -> Option<Size> {
    let sections = result.get("builder_result")?.get("executable_sections_size")?.as_array()?;
    let get = |name: &str| {
        sections.iter().find(|s| s.get("name").and_then(|n| n.as_str()) == Some(name)).and_then(|s| {
            Some((
                s.get("size")?.as_u64()? as u32,
                s.get("max_size").and_then(|m| m.as_u64()).unwrap_or(0) as u32,
            ))
        })
    };
    let (flash, flash_max) = get("text")?;
    let (ram, ram_max) = get("data").unwrap_or((0, 0));
    Some(Size { flash, flash_max, ram, ram_max })
}

/// Reads `arduino-cli compile --format json`'s answer; the `.hex` comes from `output_dir`.
pub fn read_compile_output(json: &serde_json::Value, job: &CompileJob) -> Result<Compiled, ToolError> {
    let text = |k: &str| json.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let name = job.sketch_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let hex = if success {
        let file = job.output_dir.join(format!("{name}.ino.hex"));
        Some(std::fs::read_to_string(&file).map_err(|e| {
            ToolError(format!(
                "arduino-cli said the sketch compiled, but {} can't be read: {e}",
                file.display()
            ))
        })?)
    } else {
        None
    };
    let error = Some(text("error")).filter(|e| !e.is_empty());
    let mut output = text("compiler_err");
    let out = text("compiler_out");
    if !success && !out.is_empty() {
        output = format!("{out}{output}");
    }
    Ok(Compiled { success, hex, output, size: size_of(json), error })
}

impl Toolchain for ArduinoCli {
    fn info(&self) -> ToolchainInfo {
        let version = self
            .run_json(&["version"])
            .ok()
            .and_then(|v| v.get("VersionString").and_then(|s| s.as_str()).map(str::to_string));
        ToolchainInfo { path: self.exe.clone(), source: self.source, version }
    }

    fn has_avr_core(&self) -> Result<bool, ToolError> {
        let v = self.run_json(&["core", "list"])?;
        let platforms = v.get("platforms").and_then(|p| p.as_array()).cloned().unwrap_or_default();
        Ok(platforms.iter().any(|p| p.get("id").and_then(|i| i.as_str()) == Some("arduino:avr")))
    }

    fn install_avr_core(&self, line: &mut dyn FnMut(&str)) -> Result<(), ToolError> {
        for args in [&["core", "update-index"][..], &["core", "install", "arduino:avr"][..]] {
            let out = self.command().args(args).output().map_err(|e| self.cant_run(e))?;
            for l in String::from_utf8_lossy(&out.stdout)
                .lines()
                .chain(String::from_utf8_lossy(&out.stderr).lines())
            {
                line(l);
            }
            if !out.status.success() {
                return Err(ToolError(format!(
                    "arduino-cli {} failed: {}",
                    args.join(" "),
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        }
        Ok(())
    }

    fn compile(&self, job: &CompileJob) -> Result<Compiled, ToolError> {
        let define = format!("-DFABER_BUILD_ID=0x{:016x}ULL", job.build_id);
        let mut c = self.command();
        c.arg("compile")
            .arg("--fqbn")
            .arg(&job.fqbn)
            .arg("--build-path")
            .arg(&job.build_dir)
            .arg("--output-dir")
            .arg(&job.output_dir)
            .arg("--build-property")
            .arg(format!("compiler.cpp.extra_flags={define}"))
            .arg("--build-property")
            .arg(format!("compiler.c.extra_flags={define}"))
            .args(["--warnings", "default", "--format", "json"]);
        for lib in &job.libraries {
            c.arg("--library").arg(lib);
        }
        c.arg(&job.sketch_dir);
        let out = c.output().map_err(|e| self.cant_run(e))?;
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| {
            ToolError(format!(
                "arduino-cli compile answered something Faber can't read ({e}): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        })?;
        read_compile_output(&json, job)
    }
}

/// Finds the real `arduino-cli` on this computer, each time it is asked.
pub struct SystemFinder {
    pub data_dir: PathBuf,
}

impl Finder for SystemFinder {
    fn find(&self) -> Option<Arc<dyn Toolchain>> {
        let (exe, source) = locate(&Places::of_this_computer(&self.data_dir))?;
        let config = (source == Source::Faber).then(|| faber_dir(&self.data_dir).join("arduino-cli.yaml"));
        Some(Arc::new(ArduinoCli { exe, source, config }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn faber_s_own_copy_wins_then_the_path_then_the_ide() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        let ide = r.join("local/Programs/Arduino IDE/resources/app/lib/backend/resources").join(exe_name());
        let on_path = r.join("bin").join(exe_name());
        let own = faber_dir(&r.join("data")).join(exe_name());
        let places = Places {
            data_dir: r.join("data"),
            path: Some(std::env::join_paths([r.join("empty"), r.join("bin")]).unwrap()),
            local_app_data: Some(r.join("local")),
            program_files: Some(r.join("pf")),
            applications: None,
        };
        assert_eq!(locate(&places), None, "nothing installed");
        touch(&ide);
        assert_eq!(locate(&places), Some((ide.clone(), Source::ArduinoIde)));
        touch(&on_path);
        assert_eq!(locate(&places), Some((on_path, Source::Path)));
        touch(&own);
        assert_eq!(locate(&places), Some((own, Source::Faber)));
    }

    #[test]
    fn an_ide_installed_for_every_user_is_found_too() {
        let root = tempfile::tempdir().unwrap();
        let ide = root.path().join("pf/Arduino IDE/resources/app/lib/backend/resources").join(exe_name());
        touch(&ide);
        let places = Places {
            data_dir: root.path().join("data"),
            program_files: Some(root.path().join("pf")),
            ..Places::default()
        };
        assert_eq!(locate(&places), Some((ide, Source::ArduinoIde)));
    }

    fn job(dir: &Path) -> CompileJob {
        CompileJob {
            sketch_dir: dir.join("Ultrasonic"),
            fqbn: "arduino:avr:uno".into(),
            build_dir: dir.join("build"),
            output_dir: dir.join("out"),
            libraries: vec![],
            build_id: 1,
        }
    }

    #[test]
    fn a_real_compile_answer_gives_the_hex_and_the_size() {
        // Trimmed from arduino-cli 1.5.1's answer for the FaberLink example Ultrasonic on an Uno.
        let json: serde_json::Value = serde_json::from_str(
            r#"{"compiler_out":"Sketch uses 6822 bytes (21%) of program storage space. Maximum is 32256 bytes.\n",
                "compiler_err":"",
                "builder_result":{"executable_sections_size":[{"name":"text","size":6822,"max_size":32256},
                                                              {"name":"data","size":589,"max_size":2048}]},
                "success":true}"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("out/Ultrasonic.ino.hex"));
        std::fs::write(dir.path().join("out/Ultrasonic.ino.hex"), ":00000001FF\n").unwrap();
        let c = read_compile_output(&json, &job(dir.path())).unwrap();
        assert!(c.success);
        assert_eq!(c.hex.as_deref(), Some(":00000001FF\n"));
        assert_eq!(c.size, Some(Size { flash: 6822, flash_max: 32256, ram: 589, ram_max: 2048 }));
        assert_eq!(c.error, None);
    }

    #[test]
    fn a_failed_compile_keeps_the_compilers_words() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"compiler_out":"","compiler_err":"C:\\W\\Bad\\Bad.ino:5:1: error: expected ';'\r\n",
                "builder_result":{},"upload_result":{},"success":false,"error":"Error during build: exit status 1"}"#,
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let c = read_compile_output(&json, &job(dir.path())).unwrap();
        assert!(!c.success);
        assert_eq!(c.hex, None);
        assert!(c.output.contains("error: expected ';'"));
        assert_eq!(c.error.as_deref(), Some("Error during build: exit status 1"));
        assert_eq!(c.size, None);
    }
}
