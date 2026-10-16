//! The Arduino editor's types (docs/api.md, "The Arduino editor"). Part of [`super::dto`].

use super::FlashResult;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `GET /api/v0/arduino`: whether the editor can compile, and what it offers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArduinoInfo {
    pub tools: ArduinoTools,
    pub boards: Vec<ArduinoBoard>,
    pub templates: Vec<SketchTemplate>,
    /// Where new sketches go: `home:/Arduino`.
    pub sketchbook: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArduinoTools {
    pub state: ArduinoToolsState,
    /// For `broken`: what is wrong.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The `arduino-cli` found, if one was.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli: Option<ArduinoCli>,
    /// "Install Arduino tools" is running.
    pub installing: bool,
    /// What installing downloads. Missing where Arduino publishes no `arduino-cli` for the
    /// computer.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<ArduinoInstallPlan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ArduinoToolsState {
    Ready,
    /// `arduino-cli` is there, the Arduino AVR core isn't.
    NoAvrCore,
    /// No `arduino-cli` found.
    Missing,
    /// `arduino-cli` was found but doesn't work.
    Broken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArduinoCli {
    pub path: String,
    pub source: ArduinoCliSource,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Where the `arduino-cli` was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ArduinoCliSource {
    /// Installed by Faber in `<data>/tools/arduino-cli/`.
    Faber,
    /// On the `PATH`.
    Path,
    /// Inside the Arduino IDE 2.
    ArduinoIde,
    /// The core runs with `--fake-arduino-cli` (tests).
    Fake,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArduinoInstallPlan {
    pub cli_version: String,
    /// The release file, from Arduino's GitHub releases.
    pub url: String,
    /// The board package `arduino-cli` installs afterwards: `arduino:avr`.
    pub core: String,
    /// About how much is downloaded in all.
    pub approx_mb: u32,
}

/// A board the editor compiles for: the flasher's boards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ArduinoBoard {
    pub id: String,
    pub name: String,
    pub chip: String,
    /// What `arduino-cli` calls it.
    pub fqbn: String,
    /// The flash a program may use, in bytes.
    pub room: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SketchTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    /// The sketch uses FaberLink: the board shows up in Parts.
    pub faberlink: bool,
}

/// Body of `POST /api/v0/arduino/sketches`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewSketchRequest {
    pub name: String,
    /// A template id from `GET /api/v0/arduino`.
    pub template: String,
}

/// A sketch's `.ino` in Files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SketchRef {
    pub path: String,
    /// For a sketch opened from a board: a copy of what was flashed, in a new folder.
    pub copy: bool,
}

/// Body of `POST /api/v0/arduino/compile`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompileRequest {
    /// The sketch's `.ino`, another of its files, or its folder.
    pub sketch: String,
    pub board: String,
}

/// Body of `POST /api/v0/arduino/upload`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadRequest {
    pub sketch: String,
    pub board: String,
    pub port: String,
    /// Stop the FaberLink board on the port for it, as for the flasher.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_part: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CompileResult {
    pub ok: bool,
    pub board: String,
    /// The sketch's `.ino`.
    pub sketch: String,
    pub sketch_name: String,
    /// 16 hex digits, compiled in as `FABER_BUILD_ID`.
    pub build_id: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SketchSize>,
    pub diagnostics: Vec<SketchDiagnostic>,
    /// What went wrong when no error names a file.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SketchSize {
    pub flash: u32,
    pub flash_max: u32,
    pub ram: u32,
    pub ram_max: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SketchDiagnostic {
    pub severity: DiagnosticSeverity,
    /// In the sketch folder (`Blink.ino`, `src/motor.cpp`), or a library file's name.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// `file` is one of the sketch's own files.
    pub in_sketch: bool,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UploadResult {
    pub compile: CompileResult,
    /// Missing when the compile failed (nothing was flashed).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flashed: Option<FlashResult>,
    /// What Faber recorded. Missing if it couldn't (the log says why).
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<FlashRecord>,
}

/// Which board it is, as its USB port tells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardIdentityInfo {
    /// The board's USB serial number.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// The port it was on.
    pub port: String,
    /// False without a serial number: an identical board on the same port can't be told apart.
    pub unique: bool,
}

/// What Faber flashed on a board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlashRecord {
    pub id: String,
    pub board: String,
    pub board_name: String,
    /// The sketch's `.ino` in Files when it was flashed.
    pub sketch: String,
    pub sketch_name: String,
    pub build_id: String,
    /// SHA-256 of the programmed bytes.
    pub sha256: String,
    pub bytes: u32,
    /// RFC 3339, UTC.
    pub flashed_at: String,
    pub identity: BoardIdentityInfo,
    /// About how long reading it back takes, in ms (it resets the board).
    pub check_ms: u32,
}

/// How it was proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "method", rename_all = "kebab-case")]
#[ts(export)]
pub enum CodeProof {
    /// The FaberLink board said its build id.
    BuildId,
    /// The program was read back and compared.
    #[serde(rename_all = "camelCase")]
    ReadBack {
        #[ts(type = "number")]
        elapsed_ms: u64,
    },
}

/// What is on a board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "result", rename_all = "kebab-case")]
#[ts(export)]
pub enum CodeCheck {
    /// The board runs exactly this flash.
    Same { record: FlashRecord, proof: CodeProof },
    /// The board runs other code now; `record` is the last thing Faber flashed on it.
    Different { record: FlashRecord, proof: CodeProof },
    /// Only reading it back can tell (`POST …/check`).
    NeedsCheck { record: FlashRecord },
    /// Faber has never flashed this board.
    Never,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardCodeInfo {
    pub port: String,
    pub identity: BoardIdentityInfo,
    pub code: CodeCheck,
}

/// Body of `POST /api/v0/arduino/boards/{port}/check`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckCodeRequest {
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_part: Option<bool>,
}

/// Body of `POST /api/v0/arduino/flashed/{id}/open`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpenFlashedRequest {
    /// Always make a copy, even if the sketch in Files is still what was flashed.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<bool>,
}

/// Where "Install Arduino tools" is (the `arduino-tools` topic).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ArduinoToolsStep {
    Downloading,
    Verifying,
    Unpacking,
    InstallingCore,
    Done,
}

/// A FaberLink board's code, as it says it (on its bridge in Parts).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BoardCodeLink {
    /// The build id the board says, if it says one.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
    /// The flash from Faber with that build id.
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<FlashRecord>,
}
