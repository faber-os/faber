//! The Arduino editor (docs/arduino-editor.md): new sketches in Files, compiling with
//! `arduino-cli`, uploading with Faber's own flasher, and "what's on this board".
//!
//! After every upload Faber records the board, the sketch, an exact copy of its files and the
//! program ([`crate::arduino::records`]). A record is never taken as what the board runs *now*:
//! [`Machine::board_code`] proves it with the build id a FaberLink board says in its
//! `HELLO_ACK`, and [`Machine::check_board_code`] by reading the program back through the
//! bootloader. Without one of those two, the answer is "Faber can't tell yet".

use super::flash::{FirmwareSource, FlashPort, FlashRequest, Flashed, Use, from_flash};
use super::{ErrorKind, Machine, MachineError};
use crate::arduino::diagnostics::{self, Diagnostic};
use crate::arduino::fake::{FakeFinder, FakeToolchain};
use crate::arduino::install::{self, Downloader, HttpDownloader, InstallError, Release, Step};
use crate::arduino::library::{self, Template};
use crate::arduino::records::{BoardIdentity, Flash, Record, Records};
use crate::arduino::sketch::{self, CollectError, Sources};
use crate::arduino::toolchain::{CompileJob, Finder, Size, SystemFinder, Toolchain, ToolchainInfo};
use crate::config::BRIDGE_CONNECTOR;
use crate::files::{RootKey, VPath};
use crate::ports::{Claimant, PortEntry, PortId};
use faber_flash::{Board, Image};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use tokio::sync::broadcast;

/// The claim on a port while the editor reads a program back: `tool:arduino-editor`.
pub const EDITOR: &str = "arduino-editor";

/// Where sketches live: `home:/Arduino/<name>/<name>.ino`, as the Arduino IDE keeps them.
pub const SKETCHBOOK: &str = "Arduino";

/// The editor's share of the machine.
pub(super) struct Workshop {
    finder: RwLock<Arc<dyn Finder>>,
    downloader: RwLock<Arc<dyn Downloader>>,
    /// A leaf lock: nothing else is taken while it is held.
    records: Mutex<Records>,
    /// One compile at a time: `arduino-cli` is heavy on a Pi, and the build folders are shared.
    compiling: Mutex<()>,
    installing: Mutex<()>,
    /// One guided board's plan change at a time (machine/wiring.rs). A leaf lock.
    pub(super) plans: Mutex<()>,
    events: broadcast::Sender<ToolsProgress>,
}

impl Workshop {
    pub(super) fn new(data_dir: &Path) -> Workshop {
        Workshop {
            finder: RwLock::new(Arc::new(SystemFinder { data_dir: data_dir.to_path_buf() })),
            downloader: RwLock::new(Arc::new(HttpDownloader)),
            records: Mutex::new(Records::open(&data_dir.join("arduino"))),
            compiling: Mutex::new(()),
            installing: Mutex::new(()),
            plans: Mutex::new(()),
            events: broadcast::channel(64).0,
        }
    }
}

/// Whether the editor can compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolsState {
    Ready,
    /// `arduino-cli` is there, but not the AVR core.
    NoAvrCore,
    /// No `arduino-cli` was found.
    Missing,
    /// `arduino-cli` was found but doesn't work: why.
    Broken(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArduinoStatus {
    pub state: ToolsState,
    pub toolchain: Option<ToolchainInfo>,
    /// "Install Arduino tools" is running.
    pub installing: bool,
    /// What installing would download, for the confirm dialog. `None` where Arduino publishes no
    /// `arduino-cli` for this computer.
    pub release: Option<Release>,
    pub approx_download_mb: u32,
}

/// Progress of "Install Arduino tools", for the `arduino-tools` topic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolsProgress {
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    Verifying,
    Unpacking,
    /// `arduino-cli core install arduino:avr`; `line` is what it printed last.
    InstallingCore {
        line: String,
    },
    Done,
}

/// A compile, as the editor shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileReport {
    pub ok: bool,
    pub board: &'static Board,
    /// The sketch's `.ino` in Files.
    pub sketch: String,
    pub sketch_name: String,
    pub build_id: String,
    pub size: Option<Size>,
    pub diagnostics: Vec<Diagnostic>,
    /// What went wrong when the compiler named no file (a missing board package, …).
    pub message: Option<String>,
    sources: Sources,
    image: Option<Image>,
    hex: Option<String>,
}

impl CompileReport {
    /// The program as Intel HEX, if the compile worked.
    pub fn hex(&self) -> Option<&str> {
        if self.ok { self.hex.as_deref() } else { None }
    }
}

/// An upload: the compile, then (if it worked) the flash and the record of it.
#[derive(Clone, Debug, PartialEq)]
pub struct UploadReport {
    pub compile: CompileReport,
    pub flashed: Option<Flashed>,
    /// `None` if the flash worked but Faber couldn't write down what it flashed (logged).
    pub record: Option<Record>,
}

/// How a board's code was proved to be (or not to be) a recorded flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof {
    /// The FaberLink board said its build id: live, and nothing was reset.
    BuildId,
    /// The program was read back through the bootloader and compared.
    ReadBack { elapsed_ms: u64 },
}

/// What is on a board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeResult {
    /// The board runs exactly this recorded flash.
    Same { record: Record, proof: Proof },
    /// The board runs something else now; `record` is the last thing Faber flashed on it.
    Different { record: Record, proof: Proof },
    /// Faber flashed this board, but nothing proves what runs now without reading it back.
    NeedsReadBack { record: Record },
    /// Faber has no record of flashing this board.
    Never,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardCode {
    pub port: String,
    pub identity: BoardIdentity,
    pub result: CodeResult,
}

/// A sketch to open in the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedSketch {
    /// Its `.ino` in Files.
    pub path: String,
    /// A new folder with the flashed files, because the sketch in Files has changed since (or a
    /// copy was asked for).
    pub copy: bool,
}

/// What a FaberLink board on a port says about its code, for Parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlashedCode {
    pub build_id: Option<String>,
    /// The record whose build id the board says.
    pub record: Option<Record>,
}

fn bad(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> MachineError {
    MachineError::new(ErrorKind::BadRequest, code, message, hint)
}

fn internal(what: &str, e: impl std::fmt::Display) -> MachineError {
    MachineError::new(
        ErrorKind::Internal,
        "io-error",
        format!("Faber could not {what}: {e}"),
        "Check that the disk isn't full. The log has details.",
    )
}

fn from_install(e: InstallError) -> MachineError {
    let kind = match e.code {
        "io-error" => ErrorKind::Internal,
        "download-failed" => ErrorKind::Conflict,
        _ => ErrorKind::Unprocessable,
    };
    MachineError::new(kind, e.code, e.message, e.hint)
}

/// The page-padded bytes `image` is written in on `board`, exactly as the flasher writes them:
/// what the recorded SHA-256 is taken over, and what reading back returns.
pub fn programmed_bytes(image: &Image, board: &Board) -> Vec<u8> {
    image.pages(board.page_bytes).iter().flat_map(|&a| image.page(a, board.page_bytes)).collect()
}

/// The board on a port, as well as its USB ids tell.
pub fn identity_of(entry: &PortEntry) -> BoardIdentity {
    BoardIdentity::new(
        &entry.id.to_string(),
        entry.usb.as_ref().map(|u| (u.vid, u.pid, u.serial_number.as_deref())),
    )
}

impl Machine {
    /// Uses this toolchain finder instead of the real `arduino-cli` (tests; `--fake-arduino-cli`).
    pub fn use_arduino_finder(&self, finder: Arc<dyn Finder>) {
        *self.arduino.finder.write().expect("finder lock") = finder;
    }

    /// The core's `--fake-arduino-cli`: a stand-in that compiles every sketch to a made-up program.
    pub fn use_fake_arduino_cli(&self) {
        self.use_arduino_finder(Arc::new(FakeFinder {
            toolchain: FakeToolchain::new(true),
            installed_in: None,
        }));
    }

    /// Downloads through this instead of HTTPS (tests).
    pub fn use_arduino_downloader(&self, downloader: Arc<dyn Downloader>) {
        *self.arduino.downloader.write().expect("downloader lock") = downloader;
    }

    pub fn arduino_events(&self) -> broadcast::Receiver<ToolsProgress> {
        self.arduino.events.subscribe()
    }

    fn arduino_dir(&self) -> PathBuf {
        self.data_dir.join("arduino")
    }

    fn toolchain(&self) -> Option<Arc<dyn Toolchain>> {
        let finder = Arc::clone(&self.arduino.finder.read().expect("finder lock"));
        finder.find()
    }

    pub fn arduino_status(&self) -> ArduinoStatus {
        let installing = self.arduino.installing.try_lock().is_err();
        let (state, toolchain) = match self.toolchain() {
            None => (ToolsState::Missing, None),
            Some(t) => {
                let state = match t.has_avr_core() {
                    Ok(true) => ToolsState::Ready,
                    Ok(false) => ToolsState::NoAvrCore,
                    Err(e) => ToolsState::Broken(e.0),
                };
                (state, Some(t.info()))
            }
        };
        ArduinoStatus {
            state,
            toolchain,
            installing,
            release: Release::for_this_computer(),
            approx_download_mb: install::APPROX_DOWNLOAD_MB,
        }
    }

    /// "Install Arduino tools": `arduino-cli` if none is found, then the AVR core if it is
    /// missing. Only ever on a request the user confirmed; the `arduino-tools` topic follows it.
    pub fn install_arduino_tools(&self) -> Result<ArduinoStatus, MachineError> {
        let Ok(_installing) = self.arduino.installing.try_lock() else {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-installing",
                "The Arduino tools are being installed already.",
                "Wait for it to finish; the Arduino editor shows how far it is.",
            ));
        };
        let send = |p: ToolsProgress| {
            let _ = self.arduino.events.send(p);
        };
        if self.toolchain().is_none() {
            let release = Release::for_this_computer().ok_or_else(|| {
                MachineError::new(
                    ErrorKind::Unprocessable,
                    "no-arduino-cli-for-this-computer",
                    format!(
                        "Arduino publishes no arduino-cli for this computer ({} on {}).",
                        std::env::consts::OS,
                        std::env::consts::ARCH
                    ),
                    "Install arduino-cli yourself and put it on the PATH; Faber finds it there.",
                )
            })?;
            let downloader = Arc::clone(&self.arduino.downloader.read().expect("downloader lock"));
            tracing::info!(target: "core::arduino", "installing arduino-cli {} from {}", release.version, release.archive_url);
            install::install_cli(downloader.as_ref(), &self.data_dir, &release, &mut |step| {
                send(match step {
                    Step::Downloading { done, total } => ToolsProgress::Downloading { done, total },
                    Step::Verifying => ToolsProgress::Verifying,
                    Step::Unpacking => ToolsProgress::Unpacking,
                })
            })
            .map_err(|e| {
                tracing::warn!(target: "core::arduino", "installing arduino-cli failed: {} {}", e.message, e.hint);
                from_install(e)
            })?;
        }
        let toolchain = self.toolchain().ok_or_else(|| {
            internal("find the arduino-cli it just installed", "it isn't in <data>/tools/arduino-cli")
        })?;
        let has_core = toolchain.has_avr_core().map_err(|e| tools_broken(&toolchain.info(), &e.0))?;
        if !has_core {
            tracing::info!(target: "core::arduino", "installing the Arduino AVR core");
            send(ToolsProgress::InstallingCore { line: String::new() });
            toolchain
                .install_avr_core(&mut |line| send(ToolsProgress::InstallingCore { line: line.to_string() }))
                .map_err(|e| {
                    MachineError::new(
                        ErrorKind::Conflict,
                        "core-install-failed",
                        format!("Installing the Arduino AVR core failed: {}", e.0),
                        "Check that this machine is on the internet, then try again.",
                    )
                })?;
        }
        send(ToolsProgress::Done);
        drop(_installing);
        Ok(self.arduino_status())
    }

    /// A new sketch from a template: `home:/Arduino/<name>/<name>.ino`. Returns its path.
    pub fn new_sketch(&self, name: &str, template: &str) -> Result<String, MachineError> {
        sketch::check_name(name).map_err(|why| {
            bad("invalid-sketch-name", why, "Try a name like Rover or motor_test: no spaces.")
        })?;
        let template = Template::by_id(template).ok_or_else(|| {
            bad(
                "unknown-template",
                format!("There is no sketch template called \"{template}\"."),
                "GET /api/v0/arduino lists the templates.",
            )
        })?;
        let folder = VPath::home(&[SKETCHBOOK, name]);
        let local = self.files.local_path(&folder)?;
        if local.exists() {
            return Err(MachineError::new(
                ErrorKind::Conflict,
                "already-exists",
                format!("There is already a sketch called {name}."),
                "Pick another name, or open it from Files (in the Arduino folder).",
            ));
        }
        std::fs::create_dir_all(&local).map_err(|e| internal("make the sketch's folder", e))?;
        std::fs::write(local.join(format!("{name}.ino")), template.text(name))
            .map_err(|e| internal("write the sketch", e))?;
        self.notify_folders(&[VPath::home(&[SKETCHBOOK]), folder.clone()]);
        Ok(folder.join(&format!("{name}.ino")).to_string())
    }

    /// The sketch a path is in: the `.ino`, any other file of the sketch, or its folder.
    fn sketch_at(&self, path: &str) -> Result<(VPath, String, PathBuf), MachineError> {
        let p = VPath::parse(path)?;
        let not_a_sketch = || {
            bad(
                "not-a-sketch",
                format!(
                    "\"{p}\" isn't in an Arduino sketch: a sketch is a folder with a .ino file of the same name, like Blink/Blink.ino."
                ),
                "Make a new sketch in the Arduino editor, or rename the folder to match its .ino file.",
            )
        };
        if p.root != RootKey::Home {
            return Err(bad(
                "not-in-home",
                format!("\"{p}\" isn't in Faber's home folder."),
                "Keep sketches in the Arduino folder of your home folder.",
            ));
        }
        let local = self.files.local_path(&p)?;
        let (folder, local_folder) = if local.is_dir() {
            (p.clone(), local)
        } else {
            let parent = p.parent().ok_or_else(not_a_sketch)?;
            (parent, local.parent().map(Path::to_path_buf).ok_or_else(not_a_sketch)?)
        };
        let name = folder.name().ok_or_else(not_a_sketch)?.to_string();
        if !local_folder.join(format!("{name}.ino")).is_file() {
            return Err(not_a_sketch());
        }
        sketch::check_name(&name).map_err(|why| {
            bad(
                "invalid-sketch-name",
                format!("The sketch \"{name}\" can't be compiled: {why}"),
                "Rename the folder and its .ino file (the same name for both).",
            )
        })?;
        Ok((folder, name, local_folder))
    }

    fn ready_toolchain(&self) -> Result<Arc<dyn Toolchain>, MachineError> {
        let t = self.toolchain().ok_or_else(|| {
            MachineError::new(
                ErrorKind::Conflict,
                "arduino-tools-missing",
                "The Arduino tools aren't installed on this machine yet.",
                format!(
                    "Press Install Arduino tools in the Arduino editor: Faber downloads arduino-cli and the AVR core (about {} MB), once.",
                    install::APPROX_DOWNLOAD_MB
                ),
            )
        })?;
        match t.has_avr_core() {
            Ok(true) => Ok(t),
            Ok(false) => Err(MachineError::new(
                ErrorKind::Conflict,
                "arduino-core-missing",
                "arduino-cli is installed, but not the Arduino AVR core (the compiler for the Uno, Nano and Mega).",
                "Press Install Arduino tools in the Arduino editor; it installs the AVR core.",
            )),
            Err(e) => Err(tools_broken(&t.info(), &e.0)),
        }
    }

    /// Compiles a sketch for a board (`POST /api/v0/arduino/compile`).
    pub fn compile_sketch(&self, path: &str, board: &str) -> Result<CompileReport, MachineError> {
        let board = Board::by_id(board).ok_or_else(|| {
            bad(
                "unknown-board",
                format!("The Arduino editor doesn't know the board \"{board}\"."),
                "Pick one of: uno, nano, nano-old, mega2560.",
            )
        })?;
        let fqbn = crate::arduino::fqbn(board.id).expect("every flasher board has an FQBN");
        let (folder, name, local) = self.sketch_at(path)?;
        self.check_plan_board(&local, &name, board.id)?;
        let toolchain = self.ready_toolchain()?;
        let sources = Sources::collect(&local).map_err(|e| match e {
            CollectError::Io(e) => internal("read the sketch", e),
            CollectError::TooBig => MachineError::new(
                ErrorKind::Unprocessable,
                "sketch-too-big",
                format!(
                    "The sketch {name} has more than {} files or {} MB of code.",
                    sketch::MAX_FILES,
                    sketch::MAX_BYTES / 1024 / 1024
                ),
                "Keep only the sketch's own tabs in its folder; libraries go elsewhere.",
            ),
        })?;
        let build_id = sources.build_id(fqbn);
        let _compiling = self.arduino.compiling.lock().expect("compile lock");
        let dir = self.arduino_dir();
        // The exact files that are hashed are the ones compiled: a copy, not the Files folder.
        let work = dir.join("work").join(&name);
        let out = dir.join("out").join(&name);
        let prepare = || -> std::io::Result<PathBuf> {
            for d in [&work, &out] {
                let _ = std::fs::remove_dir_all(d);
                std::fs::create_dir_all(d)?;
            }
            sources.write_to(&work)?;
            library::install(&dir.join("libraries"))
        };
        let faberlink = prepare().map_err(|e| internal("prepare the sketch for compiling", e))?;
        let job = CompileJob {
            sketch_dir: work.clone(),
            fqbn: fqbn.to_string(),
            build_dir: dir.join("build").join(board.id).join(&name),
            output_dir: out,
            libraries: vec![faberlink],
            build_id,
        };
        tracing::info!(target: "core::arduino", "compiling {folder} for {}", board.name);
        let compiled = toolchain.compile(&job).map_err(|e| tools_broken(&toolchain.info(), &e.0))?;
        let diagnostics = diagnostics::parse(&compiled.output, &work);
        let image = match (&compiled.hex, compiled.success) {
            (Some(hex), true) => Some(Image::parse(hex).map_err(|e| {
                internal("read the program arduino-cli made", format!("the .hex file is broken: {e}"))
            })?),
            _ => None,
        };
        let message = (!compiled.success
            && !diagnostics.iter().any(|d| d.severity == diagnostics::Severity::Error))
        .then(|| {
            compiled.error.clone().unwrap_or_else(|| {
                compiled.output.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
            })
        });
        Ok(CompileReport {
            ok: compiled.success && image.is_some(),
            board,
            sketch: folder.join(&format!("{name}.ino")).to_string(),
            sketch_name: name,
            build_id: sketch::build_id_text(build_id),
            size: compiled.size,
            diagnostics,
            message,
            sources,
            image,
            hex: compiled.hex,
        })
    }

    fn port_entry(&self, port: &str) -> Result<PortEntry, MachineError> {
        let st = self.state.lock();
        PortId::parse(port).and_then(|id| st.ports.iter().find(|p| p.id == id).cloned()).ok_or_else(|| {
            bad(
                "unknown-port",
                format!("There is no port called \"{port}\" right now."),
                "Check that the board is plugged in, then pick it from the list of ports.",
            )
        })
    }

    /// Compiles, then puts the program on the board with Faber's flasher, and records it
    /// (`POST /api/v0/arduino/upload`). A failed compile flashes nothing.
    pub fn upload_sketch(
        &self,
        path: &str,
        board: &str,
        port: &str,
        stop_part: bool,
    ) -> Result<UploadReport, MachineError> {
        let entry = self.port_entry(port)?;
        let compile = self.compile_sketch(path, board)?;
        let (Some(image), Some(hex)) = (compile.image.clone(), compile.hex.clone()) else {
            return Ok(UploadReport { compile, flashed: None, record: None });
        };
        let flashed = self.flash_port(FlashRequest {
            port: port.to_string(),
            board: Some(compile.board.id.to_string()),
            firmware: FirmwareSource::Compiled(image.clone()),
            stop_part,
        })?;
        let sha256 = sketch::sha256_hex(&programmed_bytes(&image, compile.board));
        let recorded = self.arduino.records.lock().expect("records lock").record(Flash {
            identity: identity_of(&entry),
            board: compile.board.id,
            sketch: &compile.sketch,
            sketch_name: &compile.sketch_name,
            sources: &compile.sources,
            hex: &hex,
            build_id: &compile.build_id,
            sha256: &sha256,
            bytes: image.end(),
            flashed_at: format!("{:.0}", jiff::Timestamp::now()),
        });
        let record = match recorded {
            Ok(r) => Some(r),
            Err(e) => {
                tracing::error!(target: "core::arduino", "flashed {} but could not record it: {e}", entry.label);
                None
            }
        };
        Ok(UploadReport { compile, flashed: Some(flashed), record })
    }

    /// The build id a FaberLink board that is connected on this port says, if one is: `None`
    /// when no bridge on the port has an answer from its board.
    fn live_build_id(&self, port: &PortId) -> Option<Option<u64>> {
        let st = self.state.lock();
        st.config.parts.iter().find_map(|(name, cfg)| {
            if cfg.connector != BRIDGE_CONNECTOR
                || cfg.source.port().and_then(PortId::parse).as_ref() != Some(port)
            {
                return None;
            }
            let view = super::bridge_handle(&st, name)?.view();
            let answered =
                matches!(view.link, crate::bridge::LinkState::Ready | crate::bridge::LinkState::Describing);
            answered.then(|| view.board.map(|b| b.build_id))?
        })
    }

    /// What is on the board on a port, from what can be known without touching it
    /// (`GET /api/v0/arduino/boards/{port}/code`): a connected FaberLink board's build id, else
    /// "read it back to know".
    pub fn board_code(&self, port: &str) -> Result<BoardCode, MachineError> {
        let entry = self.port_entry(port)?;
        let identity = identity_of(&entry);
        let live = self.live_build_id(&entry.id);
        let records = self.arduino.records.lock().expect("records lock");
        let mine = records.for_board(&identity).cloned();
        let result = match (live, mine) {
            (Some(Some(id)), mine) => {
                let id = sketch::build_id_text(id);
                match mine {
                    Some(r) if r.build_id == id => CodeResult::Same { record: r, proof: Proof::BuildId },
                    // The same build, flashed by Faber on another board (or before a new USB id).
                    _ if records.by_build_id(&id).is_some() => CodeResult::Same {
                        record: records.by_build_id(&id).cloned().expect("just found"),
                        proof: Proof::BuildId,
                    },
                    Some(r) => CodeResult::Different { record: r, proof: Proof::BuildId },
                    None => CodeResult::Never,
                }
            }
            // A FaberLink sketch without a build id was compiled elsewhere (the Arduino IDE).
            (Some(None), Some(r)) => CodeResult::Different { record: r, proof: Proof::BuildId },
            (Some(None), None) => CodeResult::Never,
            (None, Some(r)) => CodeResult::NeedsReadBack { record: r },
            (None, None) => CodeResult::Never,
        };
        Ok(BoardCode { port: entry.id.to_string(), identity, result })
    }

    /// Reads the recorded program back from the board and compares it
    /// (`POST /api/v0/arduino/boards/{port}/check`). It resets the board, so it only runs when
    /// the user asks; a FaberLink board on the port is stopped only with `stop_part`.
    pub fn check_board_code(&self, port: &str, stop_part: bool) -> Result<BoardCode, MachineError> {
        let entry = self.port_entry(port)?;
        let identity = identity_of(&entry);
        let found = {
            let records = self.arduino.records.lock().expect("records lock");
            records.for_board(&identity).cloned().map(|r| (records.hex(&r), r))
        };
        let Some((hex, record)) = found else {
            return Ok(BoardCode { port: entry.id.to_string(), identity, result: CodeResult::Never });
        };
        let board = Board::by_id(&record.board).ok_or_else(|| {
            internal("read boards.toml", format!("it names an unknown board \"{}\"", record.board))
        })?;
        let image = hex
            .map_err(|e| internal("read the recorded program", e))
            .and_then(|h| Image::parse(&h).map_err(|e| internal("read the recorded program", e)))?;
        let me = Claimant::Tool(EDITOR);
        let _stopped = self.claim_for_flashing(&entry, &me, stop_part, Use::ReadBack)?;
        let started = Instant::now();
        let result = match self.opener.open(&entry, board.baud) {
            Ok((reader, writer)) => {
                let mut port = FlashPort { reader, writer, start: Instant::now() };
                let id = entry.id.to_string();
                let events = self.flash_events.clone();
                tracing::info!(target: "core::arduino", "reading the program back from {} ({} bytes)", entry.label, image.end());
                faber_flash::read_back(&mut port, board, &image, &faber_flash::Timing::default(), &mut |p| {
                    let _ = events.send(super::FlashProgress { port: id.clone(), progress: p });
                })
                .map_err(|e| from_flash(&e))
            }
            Err(e) => Err(MachineError::new(
                ErrorKind::Conflict,
                "port-unavailable",
                format!("Could not open {}: {e}", entry.label),
                "Check that no other program (for example the Arduino IDE) uses it.",
            )),
        };
        {
            let mut st = self.state.lock();
            st.claims.release(&entry.id, &me);
            self.ports_changed.send_modify(|n| *n += 1);
            self.reconcile(&mut st); // a bridge stopped for this starts again
        }
        let read = result?;
        let proof = Proof::ReadBack { elapsed_ms: started.elapsed().as_millis() as u64 };
        let same = read.first_difference.is_none() && sketch::sha256_hex(&read.read) == record.sha256;
        tracing::info!(target: "core::arduino", "{} {} {}", entry.label, if same { "still runs" } else { "no longer runs" }, record.sketch_name);
        let result =
            if same { CodeResult::Same { record, proof } } else { CodeResult::Different { record, proof } };
        Ok(BoardCode { port: entry.id.to_string(), identity, result })
    }

    /// Puts a recorded flash's sketch in Files to open it (`POST /api/v0/arduino/flashed/{id}/open`):
    /// the sketch itself if it is still exactly what was flashed, else a copy of the flashed files.
    pub fn open_flashed(&self, id: &str, copy: bool) -> Result<OpenedSketch, MachineError> {
        let (record, sources) = {
            let records = self.arduino.records.lock().expect("records lock");
            let record = records.get(id).cloned().ok_or_else(|| {
                MachineError::new(
                    ErrorKind::NotFound,
                    "unknown-flash",
                    format!("Faber has no record called \"{id}\"."),
                    "Check the board again: the record may have been replaced by a newer upload.",
                )
            })?;
            let sources = records.sources(&record).map_err(|e| internal("read the flashed copy", e))?;
            (record, sources)
        };
        if !copy
            && let Ok((_, name, local)) = self.sketch_at(&record.sketch)
            && name == record.sketch_name
            && Sources::collect(&local).is_ok_and(|now| now == sources)
        {
            return Ok(OpenedSketch { path: record.sketch.clone(), copy: false });
        }
        let book = self.files.local_path(&VPath::home(&[SKETCHBOOK]))?;
        std::fs::create_dir_all(&book).map_err(|e| internal("make the Arduino folder", e))?;
        let base: String = format!("{}-on-board", record.sketch_name).chars().take(58).collect();
        let name = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
            .find(|n| !book.join(n).exists())
            .expect("a free name");
        let main = format!("{}.ino", record.sketch_name);
        let renamed = Sources(
            sources
                .0
                .into_iter()
                .map(|(rel, data)| (if rel == main { format!("{name}.ino") } else { rel }, data))
                .collect(),
        );
        std::fs::create_dir_all(book.join(&name)).map_err(|e| internal("make the copy's folder", e))?;
        renamed.write_to(&book.join(&name)).map_err(|e| internal("write the copy", e))?;
        self.notify_folders(&[VPath::home(&[SKETCHBOOK])]);
        Ok(OpenedSketch {
            path: VPath::home(&[SKETCHBOOK, &name, &format!("{name}.ino")]).to_string(),
            copy: true,
        })
    }

    /// What a FaberLink board's build id says about its code, for its line in Parts.
    pub(super) fn flashed_code(&self, build_id: Option<u64>) -> FlashedCode {
        let build_id = build_id.map(sketch::build_id_text);
        let record = build_id
            .as_deref()
            .and_then(|id| self.arduino.records.lock().expect("records lock").by_build_id(id).cloned());
        FlashedCode { build_id, record }
    }
}

fn tools_broken(info: &ToolchainInfo, why: &str) -> MachineError {
    MachineError::new(
        ErrorKind::Internal,
        "arduino-tools-broken",
        format!("arduino-cli ({}) doesn't work: {why}", info.path.display()),
        "Install the Arduino tools again from the Arduino editor, or check that program yourself.",
    )
}
