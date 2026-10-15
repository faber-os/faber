//! Package routes (docs/api.md, "Packages"): list, install (or preview) and uninstall.

use super::{AppState, Failure, blocking, convert, dto};
use crate::packages::{EXTENSION, MAX_BYTES};
use axum::Json;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use serde::Deserialize;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        // The body is read below with the package limit, so the general limit doesn't apply.
        .route("/api/v0/packages", get(list).post(install).layer(DefaultBodyLimit::disable()))
        .route("/api/v0/packages/{id}", delete(uninstall))
}

#[derive(Deserialize)]
struct InstallQuery {
    /// `preview`: check it and say what it would install, without installing.
    #[serde(default)]
    mode: Option<String>,
    /// The package file's name, remembered as where it came from.
    #[serde(default)]
    name: Option<String>,
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<dto::PackageInfo>>, Failure> {
    let list = blocking(move || s.machine.packages()).await??;
    Ok(Json(list.iter().map(convert::package).collect()))
}

async fn install(
    State(s): State<AppState>,
    Query(q): Query<InstallQuery>,
    body: Body,
) -> Result<Response, Failure> {
    let preview = match q.mode.as_deref() {
        None | Some("install") => false,
        Some("preview") => true,
        Some(other) => {
            return Err(Failure::new(
                StatusCode::BAD_REQUEST,
                "bad-request",
                format!("mode={other} is not a way to install a package."),
                "Leave mode out to install, or use mode=preview to see what it would install.",
            ));
        }
    };
    let bytes = axum::body::to_bytes(body, MAX_BYTES as usize + 1).await.map_err(|_| {
        Failure::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "package-too-big",
            format!("The package is larger than {} MB, the most Faber installs.", MAX_BYTES >> 20),
            "Leave large recordings out of captures/; a few seconds of data is enough to test with.",
        )
    })?;
    let source = q.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("upload.{EXTENSION}"));
    let (preview_of, installed) =
        blocking(move || s.machine.install_package(&bytes, &source, preview)).await??;
    Ok(match installed {
        None => Json(convert::package_preview(&preview_of)).into_response(),
        Some(view) => (StatusCode::CREATED, Json(convert::package(&view))).into_response(),
    })
}

async fn uninstall(State(s): State<AppState>, Path(id): Path<String>) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.uninstall_package(&id)).await??;
    Ok(StatusCode::NO_CONTENT)
}
