//! Files and settings routes (docs/api.md, "Files" and "Settings").

use super::{ApiJson, AppState, Failure, blocking, convert, dto};
use crate::files::MAX_FILE_BYTES;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/files/roots", get(roots))
        .route("/api/v0/files/list", get(list))
        .route(
            "/api/v0/files/content",
            get(read).put(write).layer(DefaultBodyLimit::max(MAX_FILE_BYTES as usize + 1024)),
        )
        .route("/api/v0/files/folder", post(create_folder))
        .route("/api/v0/files/move", post(move_file))
        .route("/api/v0/files/trash", post(trash))
        .route("/api/v0/files/restore", post(restore))
        .route("/api/v0/files/delete", post(delete))
        .route("/api/v0/files/empty-trash", post(empty_trash))
        .route("/api/v0/files/samples", post(samples))
        .route("/api/v0/settings", get(settings).put(update_settings))
        .route("/api/v0/settings/developer", post(change_developer))
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
    #[serde(default)]
    download: Option<u8>,
}

async fn roots(State(s): State<AppState>) -> Json<Vec<dto::FileRoot>> {
    Json(
        s.machine
            .file_roots()
            .into_iter()
            .map(|(p, label, warning)| convert::file_root(&p, label, warning))
            .collect(),
    )
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<PathQuery>,
) -> Result<Json<dto::FolderListing>, Failure> {
    let path = q.path.clone();
    let entries = blocking(move || s.machine.list_files(&path)).await??;
    Ok(Json(convert::folder_listing(&q.path, &entries)))
}

async fn read(State(s): State<AppState>, Query(q): Query<PathQuery>) -> Result<Response, Failure> {
    let path = q.path.clone();
    let (p, bytes) = blocking(move || s.machine.read_file(&path)).await??;
    let name = p.name().unwrap_or("file").to_string();
    let mut res = ([(header::CONTENT_TYPE, content_type(&name))], bytes).into_response();
    if q.download == Some(1) {
        let disposition = format!("attachment; filename=\"{}\"", name.replace('"', ""));
        res.headers_mut()
            .insert(header::CONTENT_DISPOSITION, disposition.parse().expect("ascii-safe header"));
    }
    Ok(res)
}

async fn write(
    State(s): State<AppState>,
    Query(q): Query<PathQuery>,
    body: Bytes,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.write_file(&q.path, &body)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_folder(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::PathRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.create_folder(&body.path)).await??;
    Ok(StatusCode::CREATED)
}

async fn move_file(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::MoveRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.move_file(&body.from, &body.to)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn trash(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.trash_files(&body.paths)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.restore_files(&body.paths)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.delete_files(&body.paths)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn empty_trash(State(s): State<AppState>) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.empty_trash()).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn samples(State(s): State<AppState>) -> Result<(StatusCode, Json<dto::PathRequest>), Failure> {
    let path = blocking(move || s.machine.add_samples()).await??;
    Ok((StatusCode::CREATED, Json(dto::PathRequest { path })))
}

async fn settings(State(s): State<AppState>) -> Json<dto::Settings> {
    Json(s.machine.settings())
}

async fn update_settings(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::Settings>,
) -> Result<Json<dto::Settings>, Failure> {
    Ok(Json(blocking(move || s.machine.update_settings(body)).await??))
}

async fn change_developer(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::DeveloperChange>,
) -> Result<Json<dto::Settings>, Failure> {
    Ok(Json(blocking(move || s.machine.change_developer(body)).await??))
}

/// Enough types for the browser to show or download files sensibly.
fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("txt" | "md" | "log" | "csv" | "toml" | "ini" | "cfg" | "yaml" | "yml") => {
            "text/plain; charset=utf-8"
        }
        Some("json") => "application/json",
        Some("html" | "htm") => "text/plain; charset=utf-8", // shown as text, never run
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "text/plain; charset=utf-8", // SVG can carry scripts; show as text
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}
