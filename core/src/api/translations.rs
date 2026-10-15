//! Translation routes (docs/api.md, "Translations"): the shell's language files installed on
//! this machine.

use super::{AppState, Failure, blocking, dto};
use crate::translations::{TranslationError, TranslationErrorKind};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/translations", get(list))
        .route("/api/v0/translations/{lang}", get(file))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<dto::TranslationInfo>>, Failure> {
    Ok(Json(blocking(move || s.machine.translations().list()).await?))
}

async fn file(
    State(s): State<AppState>,
    Path(lang): Path<String>,
) -> Result<Json<serde_json::Map<String, serde_json::Value>>, Failure> {
    Ok(Json(blocking(move || s.machine.translations().get(&lang)).await??))
}

impl From<TranslationError> for Failure {
    fn from(e: TranslationError) -> Failure {
        let status = match e.kind {
            TranslationErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            TranslationErrorKind::NotFound => StatusCode::NOT_FOUND,
            TranslationErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
        };
        Failure::new(status, e.code, e.message, e.hint)
    }
}
