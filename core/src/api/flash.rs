//! The Arduino flasher (docs/api.md, "The Arduino flasher").

use super::{ApiJson, AppState, Failure, blocking, dto};
use crate::machine::{FirmwareSource, FlashRequest, Flashed};
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/v0/flasher/boards", get(boards)).route("/api/v0/ports/{id}/flash", post(flash))
}

async fn boards() -> Json<Vec<dto::FlashBoard>> {
    Json(
        faber_flash::BOARDS
            .iter()
            .map(|b| dto::FlashBoard {
                id: b.id.into(),
                name: b.name.into(),
                chip: b.chip.into(),
                room: b.room,
            })
            .collect(),
    )
}

/// Puts a program on the board on the port. Answers when it is done: seconds for an Uno, up to
/// a minute for a whole Mega; the `flash` topic follows it page by page.
async fn flash(
    State(s): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<dto::FlashRequest>,
) -> Result<Json<dto::FlashResult>, Failure> {
    let req = FlashRequest {
        port: id,
        board: body.board,
        firmware: match body.firmware {
            dto::FirmwareRef::File { path } => FirmwareSource::File(path),
            dto::FirmwareRef::Package { package } => FirmwareSource::Package(package),
        },
        stop_part: body.stop_part.unwrap_or(false),
    };
    let f: Flashed = blocking(move || s.machine.flash_port(req)).await??;
    Ok(Json(dto::FlashResult {
        port: f.port,
        board: f.board.id.into(),
        board_name: f.board.name.into(),
        bytes: f.bytes,
        pages: f.pages,
        stopped_part: f.stopped.map(|p| p.to_string()),
    }))
}
