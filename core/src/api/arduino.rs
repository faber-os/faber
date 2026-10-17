//! The Arduino editor (docs/api.md, "The Arduino editor"; docs/arduino-editor.md).

use super::{ApiJson, AppState, Failure, blocking, dto};
use crate::arduino::diagnostics::{Diagnostic, Severity};
use crate::arduino::library::TEMPLATES;
use crate::arduino::records::{BoardIdentity, Record};
use crate::arduino::toolchain::Source;
use crate::arduino::wiring::{Candidate, PinMap, Suggestion, Template};
use crate::machine::GuidedBoard;
use crate::machine::{
    ArduinoStatus, BoardCode, CodeResult, CompileReport, Flashed, Proof, ToolsProgress, ToolsState,
    UploadReport,
};
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v0/arduino", get(info))
        .route("/api/v0/arduino/tools/install", post(install))
        .route("/api/v0/arduino/sketches", post(new_sketch))
        .route("/api/v0/arduino/compile", post(compile))
        .route("/api/v0/arduino/upload", post(upload))
        .route("/api/v0/arduino/boards/{port}/code", get(board_code))
        .route("/api/v0/arduino/boards/{port}/check", post(check_code))
        .route("/api/v0/arduino/flashed/{id}/open", post(open_flashed))
        .route("/api/v0/arduino/pins/{board}", get(pin_map))
        .route("/api/v0/arduino/parts", get(part_templates))
        .route("/api/v0/arduino/plans", get(plans).post(new_plan))
        .route("/api/v0/arduino/plans/{name}", get(plan).put(save_plan))
        .route("/api/v0/arduino/plans/{name}/suggest", post(suggest))
        .route("/api/v0/arduino/plans/{name}/take-over", post(take_over))
}

fn tools(s: &ArduinoStatus) -> dto::ArduinoTools {
    let (state, problem) = match &s.state {
        ToolsState::Ready => (dto::ArduinoToolsState::Ready, None),
        ToolsState::NoAvrCore => (dto::ArduinoToolsState::NoAvrCore, None),
        ToolsState::Missing => (dto::ArduinoToolsState::Missing, None),
        ToolsState::Broken(why) => (dto::ArduinoToolsState::Broken, Some(why.clone())),
    };
    dto::ArduinoTools {
        state,
        problem,
        cli: s.toolchain.as_ref().map(|t| dto::ArduinoCli {
            path: t.path.display().to_string(),
            source: match t.source {
                Source::Faber => dto::ArduinoCliSource::Faber,
                Source::Path => dto::ArduinoCliSource::Path,
                Source::ArduinoIde => dto::ArduinoCliSource::ArduinoIde,
                Source::Fake => dto::ArduinoCliSource::Fake,
            },
            version: t.version.clone(),
        }),
        installing: s.installing,
        install: s.release.as_ref().map(|r| dto::ArduinoInstallPlan {
            cli_version: r.version.into(),
            url: r.archive_url.clone(),
            core: "arduino:avr".into(),
            approx_mb: s.approx_download_mb,
        }),
    }
}

async fn info(State(s): State<AppState>) -> Result<Json<dto::ArduinoInfo>, Failure> {
    let status = blocking(move || s.machine.arduino_status()).await?;
    Ok(Json(dto::ArduinoInfo {
        tools: tools(&status),
        boards: faber_flash::BOARDS
            .iter()
            .map(|b| dto::ArduinoBoard {
                id: b.id.into(),
                name: b.name.into(),
                chip: b.chip.into(),
                fqbn: crate::arduino::fqbn(b.id).unwrap_or_default().into(),
                room: b.room,
            })
            .collect(),
        templates: TEMPLATES
            .iter()
            .map(|t| dto::SketchTemplate {
                id: t.id.into(),
                name: t.name.into(),
                description: t.description.into(),
                faberlink: t.faberlink,
            })
            .collect(),
        sketchbook: format!("home:/{}", crate::machine::SKETCHBOOK),
    }))
}

/// Installs `arduino-cli` and the AVR core. Answers when done (minutes); the `arduino-tools`
/// topic follows it.
async fn install(State(s): State<AppState>) -> Result<Json<dto::ArduinoTools>, Failure> {
    let status = blocking(move || s.machine.install_arduino_tools()).await??;
    Ok(Json(tools(&status)))
}

async fn new_sketch(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::NewSketchRequest>,
) -> Result<(axum::http::StatusCode, Json<dto::SketchRef>), Failure> {
    let path = blocking(move || s.machine.new_sketch(&body.name, &body.template)).await??;
    Ok((axum::http::StatusCode::CREATED, Json(dto::SketchRef { path, copy: false })))
}

fn diagnostic(d: &Diagnostic) -> dto::SketchDiagnostic {
    dto::SketchDiagnostic {
        severity: match d.severity {
            Severity::Error => dto::DiagnosticSeverity::Error,
            Severity::Warning => dto::DiagnosticSeverity::Warning,
        },
        file: d.file.clone(),
        in_sketch: d.in_sketch,
        line: d.line,
        column: d.column,
        message: d.message.clone(),
    }
}

fn compile_result(r: &CompileReport) -> dto::CompileResult {
    compile_result_with(r, false)
}

fn compile_result_with(r: &CompileReport, hex: bool) -> dto::CompileResult {
    dto::CompileResult {
        ok: r.ok,
        board: r.board.id.into(),
        sketch: r.sketch.clone(),
        sketch_name: r.sketch_name.clone(),
        build_id: r.build_id.clone(),
        size: r.size.map(|s| dto::SketchSize {
            flash: s.flash,
            flash_max: s.flash_max,
            ram: s.ram,
            ram_max: s.ram_max,
        }),
        diagnostics: r.diagnostics.iter().map(diagnostic).collect(),
        message: r.message.clone(),
        hex: if hex { r.hex().map(str::to_string) } else { None },
    }
}

fn identity(i: &BoardIdentity) -> dto::BoardIdentityInfo {
    dto::BoardIdentityInfo { serial: i.serial.clone(), port: i.port.clone(), unique: i.unique() }
}

/// About how long reading a program back takes: half a second to reset and reach the
/// bootloader, then every page at the bootloader's speed (each byte is 10 bits on the wire, plus
/// a little for the commands).
pub fn check_ms(board: &faber_flash::Board, bytes: u32) -> u32 {
    let pages = bytes.div_ceil(board.page_bytes);
    let wire_bits = u64::from(pages) * u64::from(board.page_bytes + 8) * 10;
    500 + (wire_bits * 1000 / u64::from(board.baud)) as u32
}

pub(super) fn flash_record(r: &Record) -> dto::FlashRecord {
    let board = faber_flash::Board::by_id(&r.board);
    dto::FlashRecord {
        id: r.id.clone(),
        board: r.board.clone(),
        board_name: board.map(|b| b.name.to_string()).unwrap_or_else(|| r.board.clone()),
        sketch: r.sketch.clone(),
        sketch_name: r.sketch_name.clone(),
        build_id: r.build_id.clone(),
        sha256: r.sha256.clone(),
        bytes: r.bytes,
        flashed_at: r.flashed_at.clone(),
        identity: identity(&r.identity),
        check_ms: board.map(|b| check_ms(b, r.bytes)).unwrap_or(0),
    }
}

fn flash_result(f: &Flashed) -> dto::FlashResult {
    dto::FlashResult {
        port: f.port.clone(),
        board: f.board.id.into(),
        board_name: f.board.name.into(),
        bytes: f.bytes,
        pages: f.pages,
        stopped_part: f.stopped.as_ref().map(ToString::to_string),
    }
}

fn proof(p: Proof) -> dto::CodeProof {
    match p {
        Proof::BuildId => dto::CodeProof::BuildId,
        Proof::ReadBack { elapsed_ms } => dto::CodeProof::ReadBack { elapsed_ms },
    }
}

fn board_code_info(c: &BoardCode) -> dto::BoardCodeInfo {
    dto::BoardCodeInfo {
        port: c.port.clone(),
        identity: identity(&c.identity),
        code: match &c.result {
            CodeResult::Same { record, proof: p } => {
                dto::CodeCheck::Same { record: flash_record(record), proof: proof(*p) }
            }
            CodeResult::Different { record, proof: p } => {
                dto::CodeCheck::Different { record: flash_record(record), proof: proof(*p) }
            }
            CodeResult::NeedsReadBack { record } => {
                dto::CodeCheck::NeedsCheck { record: flash_record(record) }
            }
            CodeResult::Never => dto::CodeCheck::Never,
        },
    }
}

/// Compiles. A sketch with errors is still `200`: `ok` is false and `diagnostics` say why.
async fn compile(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::CompileRequest>,
) -> Result<Json<dto::CompileResult>, Failure> {
    let hex = body.hex.unwrap_or(false);
    let r = blocking(move || s.machine.compile_sketch(&body.sketch, &body.board)).await??;
    Ok(Json(compile_result_with(&r, hex)))
}

/// Compiles, then flashes with the flasher (its errors, e.g. `409 port-used-by-bridge`), and
/// records what went on the board. The `flash` topic follows the flashing.
async fn upload(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::UploadRequest>,
) -> Result<Json<dto::UploadResult>, Failure> {
    let r: UploadReport = blocking(move || {
        s.machine.upload_sketch(&body.sketch, &body.board, &body.port, body.stop_part.unwrap_or(false))
    })
    .await??;
    Ok(Json(dto::UploadResult {
        compile: compile_result(&r.compile),
        flashed: r.flashed.as_ref().map(flash_result),
        record: r.record.as_ref().map(flash_record),
    }))
}

/// What is on the board, as far as it can be told without touching it.
async fn board_code(
    State(s): State<AppState>,
    Path(port): Path<String>,
) -> Result<Json<dto::BoardCodeInfo>, Failure> {
    let c = blocking(move || s.machine.board_code(&port)).await??;
    Ok(Json(board_code_info(&c)))
}

/// Reads the recorded program back and compares (resets the board).
async fn check_code(
    State(s): State<AppState>,
    Path(port): Path<String>,
    ApiJson(body): ApiJson<dto::CheckCodeRequest>,
) -> Result<Json<dto::BoardCodeInfo>, Failure> {
    let stop = body.stop_part.unwrap_or(false);
    let c = blocking(move || s.machine.check_board_code(&port, stop)).await??;
    Ok(Json(board_code_info(&c)))
}

async fn open_flashed(
    State(s): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<dto::OpenFlashedRequest>,
) -> Result<Json<dto::SketchRef>, Failure> {
    let copy = body.copy.unwrap_or(false);
    let o = blocking(move || s.machine.open_flashed(&id, copy)).await??;
    Ok(Json(dto::SketchRef { path: o.path, copy: o.copy }))
}

// ---- Guided boards (docs/wiring.md) ----

fn guided(g: GuidedBoard) -> dto::GuidedBoard {
    dto::GuidedBoard {
        name: g.name,
        folder: g.folder,
        sketch: g.sketch,
        plan: g.plan,
        file_problem: g.file_problem,
        problems: g.problems,
        wiring: g.wiring,
        generated: g.generated,
        edited: g.edited,
    }
}

/// A board's pins and rules: `uno`, `nano`, `nano-old`, `mega2560`, or `pi-header`.
async fn pin_map(State(s): State<AppState>, Path(board): Path<String>) -> Result<Json<PinMap>, Failure> {
    Ok(Json(s.machine.pin_map(&board)?))
}

async fn part_templates(State(s): State<AppState>) -> Json<Vec<Template>> {
    Json(s.machine.part_templates())
}

async fn plans(State(s): State<AppState>) -> Result<Json<Vec<dto::GuidedBoardSummary>>, Failure> {
    let list = blocking(move || s.machine.guided_boards()).await??;
    Ok(Json(
        list.into_iter()
            .map(|b| dto::GuidedBoardSummary {
                name: b.name,
                board: b.board,
                parts: b.parts as u32,
                taken_over: b.taken_over,
                problem: b.problem,
            })
            .collect(),
    ))
}

async fn plan(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<dto::GuidedBoard>, Failure> {
    Ok(Json(guided(blocking(move || s.machine.guided_board(&name)).await??)))
}

async fn new_plan(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::NewPlanRequest>,
) -> Result<(axum::http::StatusCode, Json<dto::GuidedBoard>), Failure> {
    let g = blocking(move || s.machine.new_guided_board(&body.name, &body.board)).await??;
    Ok((axum::http::StatusCode::CREATED, Json(guided(g))))
}

/// Saves the plan and writes the sketch; an invalid plan is `422 invalid-plan` with `problems`.
async fn save_plan(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::SavePlanRequest>,
) -> Result<Json<dto::GuidedBoard>, Failure> {
    let overwrite = body.overwrite_edits.unwrap_or(false);
    Ok(Json(guided(blocking(move || s.machine.save_plan(&name, body.plan, overwrite)).await??)))
}

/// Where a part could go; writes nothing.
async fn suggest(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<Candidate>,
) -> Result<Json<Suggestion>, Failure> {
    Ok(Json(blocking(move || s.machine.suggest_part(&name, &body)).await??))
}

async fn take_over(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<dto::GuidedBoard>, Failure> {
    Ok(Json(guided(blocking(move || s.machine.take_over_plan(&name)).await??)))
}

/// One `arduino-tools` message.
pub(super) fn tools_message(id: u32, p: &ToolsProgress) -> dto::ServerMessage {
    use dto::ArduinoToolsStep as S;
    let (step, done, total, line) = match p {
        ToolsProgress::Downloading { done, total } => (S::Downloading, Some(*done), *total, None),
        ToolsProgress::Verifying => (S::Verifying, None, None, None),
        ToolsProgress::Unpacking => (S::Unpacking, None, None, None),
        ToolsProgress::InstallingCore { line } => (S::InstallingCore, None, None, Some(line.clone())),
        ToolsProgress::Done => (S::Done, None, None, None),
    };
    dto::ServerMessage::ArduinoTools { id, step, done, total, line }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_back_an_uno_sketch_of_7_kb_takes_about_a_second() {
        let uno = faber_flash::Board::by_id("uno").unwrap();
        let ms = check_ms(uno, 7 * 1024);
        assert!((900..=1400).contains(&ms), "{ms}");
        let mega = faber_flash::Board::by_id("mega2560").unwrap();
        assert!(check_ms(mega, 250_000) > 20_000, "a full Mega takes much longer");
    }
}
