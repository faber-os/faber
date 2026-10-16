//! Files, storage, backups and settings routes (docs/api.md, "Files", "Storage and backups" and
//! "Settings").

use super::access::{FileUse, path_need, settings_need};
use super::auth::Caller;
use super::{ApiJson, AppState, Failure, blocking, convert, dto};
use crate::files::{MAX_FILE_BYTES, WriteMode};
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json};
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
        .route("/api/v0/storage", get(storage))
        .route("/api/v0/backups", post(create_backup))
        .route("/api/v0/backups/restore", post(restore_backup))
        .route("/api/v0/backups/setup", post(share_setup))
        .route("/api/v0/settings", get(settings).put(update_settings))
        .route("/api/v0/settings/reset", post(reset_settings))
        .route("/api/v0/settings/developer", post(change_developer))
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
    #[serde(default)]
    download: Option<u8>,
    /// `create`: refuse with `already-exists` instead of overwriting (uploads).
    #[serde(default)]
    mode: Option<String>,
}

async fn roots(State(s): State<AppState>) -> Json<Vec<dto::FileRoot>> {
    Json(
        s.machine
            .file_roots()
            .into_iter()
            .map(|(p, label, warning)| convert::file_root(&p, &label, warning))
            .collect(),
    )
}

/// Refused unless the caller may `use_` every one of `paths` (docs/auth.md: the whole machine's
/// files and Backups are for admins).
fn may(caller: &Caller, use_: FileUse, paths: &[&str]) -> Result<(), Failure> {
    paths.iter().try_for_each(|p| caller.require(path_need(p, use_)))
}

async fn list(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<PathQuery>,
) -> Result<Json<dto::FolderListing>, Failure> {
    may(&caller, FileUse::Read, &[&q.path])?;
    let path = q.path.clone();
    let entries = blocking(move || s.machine.list_files(&path)).await??;
    Ok(Json(convert::folder_listing(&q.path, &entries)))
}

/// Inline reads (Notes, previews) load the file and are capped at 16 MB. Downloads stream from
/// disk in chunks, so a backup with recordings of any size can be saved.
async fn read(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<PathQuery>,
) -> Result<Response, Failure> {
    may(&caller, FileUse::Read, &[&q.path])?;
    let path = q.path.clone();
    if q.download != Some(1) {
        let (p, bytes) = blocking(move || s.machine.read_file(&path)).await??;
        let name = p.name().unwrap_or("file").to_string();
        return Ok(([(header::CONTENT_TYPE, content_type(&name))], bytes).into_response());
    }
    let (p, real, len) = blocking(move || s.machine.download_file(&path)).await??;
    let file = tokio::fs::File::open(&real).await.map_err(|e| {
        Failure::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "io-error",
            format!("{p} can't be opened: {e}"),
            "Try again. If it keeps happening, look in the Logs tool.",
        )
    })?;
    let name = p.name().unwrap_or("file").to_string();
    let disposition = attachment(&name);
    Ok((
        [
            (header::CONTENT_TYPE, content_type(&name).to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from_stream(chunks(file)),
    )
        .into_response())
}

/// `Content-Disposition` for a download that keeps any file name, e.g. `kørsel.fbcap`: an ASCII
/// fallback plus the exact UTF-8 name (RFC 6266 / RFC 5987). Header values must be ASCII.
fn attachment(name: &str) -> String {
    let safe = |c: char| (c.is_ascii_graphic() || c == ' ') && !matches!(c, '"' | '\\');
    let fallback: String = name.chars().map(|c| if safe(c) { c } else { '_' }).collect();
    let mut exact = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            exact.push(b as char);
        } else {
            exact.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{exact}")
}

/// The file as a stream of 64 KiB chunks.
fn chunks(
    file: tokio::fs::File,
) -> impl futures_util::Stream<Item = std::io::Result<Bytes>> + Send + 'static {
    futures_util::stream::try_unfold(file, |mut file| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 64 * 1024];
        let n = file.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.truncate(n);
        Ok(Some((Bytes::from(buf), file)))
    })
}

async fn write(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    Query(q): Query<PathQuery>,
    body: Bytes,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &[&q.path])?;
    let mode = match q.mode.as_deref() {
        None | Some("replace") => WriteMode::Replace,
        Some("create") => WriteMode::CreateOnly,
        Some(other) => {
            return Err(Failure::new(
                StatusCode::BAD_REQUEST,
                "bad-request",
                format!("Unknown write mode \"{other}\"."),
                "Use mode=create, mode=replace, or leave it out.",
            ));
        }
    };
    blocking(move || s.machine.write_file(&q.path, &body, mode)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_folder(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::PathRequest>,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &[&body.path])?;
    blocking(move || s.machine.create_folder(&body.path)).await??;
    Ok(StatusCode::CREATED)
}

async fn move_file(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::MoveRequest>,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &[&body.from, &body.to])?;
    blocking(move || s.machine.move_file(&body.from, &body.to)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn trash(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &body.paths.iter().map(String::as_str).collect::<Vec<_>>())?;
    blocking(move || s.machine.trash_files(&body.paths)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn restore(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &body.paths.iter().map(String::as_str).collect::<Vec<_>>())?;
    blocking(move || s.machine.restore_files(&body.paths)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::PathsRequest>,
) -> Result<StatusCode, Failure> {
    may(&caller, FileUse::Change, &body.paths.iter().map(String::as_str).collect::<Vec<_>>())?;
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

async fn storage(State(s): State<AppState>) -> Result<Json<dto::StorageInfo>, Failure> {
    let st = blocking(move || s.machine.storage()).await??;
    Ok(Json(dto::StorageInfo {
        recordings_bytes: st.recordings,
        documents_bytes: st.documents,
        desktop_bytes: st.desktop,
        backups_bytes: st.backups,
        free_bytes: st.free,
        total_bytes: st.total,
    }))
}

async fn create_backup(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::BackupRequest>,
) -> Result<(StatusCode, Json<dto::BackupInfo>), Failure> {
    let (path, bytes) = blocking(move || s.machine.create_backup(body.include_recordings)).await??;
    Ok((StatusCode::CREATED, Json(dto::BackupInfo { path, bytes })))
}

/// Saves this machine's setup (a backup without the user's files or the machine's name).
async fn share_setup(State(s): State<AppState>) -> Result<(StatusCode, Json<dto::BackupInfo>), Failure> {
    let (path, bytes) = blocking(move || s.machine.share_setup()).await??;
    Ok((StatusCode::CREATED, Json(dto::BackupInfo { path, bytes })))
}

/// Keeps this machine's users unless `users` asks for the backup's (docs/auth.md, "Backups").
async fn restore_backup(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::RestoreRequest>,
) -> Result<StatusCode, Failure> {
    blocking(move || {
        if body.users {
            s.machine.restore_backup_with_users(&body.path)
        } else {
            s.machine.restore_backup(&body.path)
        }
    })
    .await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_settings(State(s): State<AppState>) -> Result<Json<dto::Settings>, Failure> {
    Ok(Json(blocking(move || s.machine.reset_settings()).await??))
}

async fn settings(State(s): State<AppState>) -> Json<dto::Settings> {
    Json(s.machine.settings())
}

/// Display settings need an operator; the machine's name, time and screen role an admin.
async fn update_settings(
    State(s): State<AppState>,
    Extension(caller): Extension<Caller>,
    ApiJson(body): ApiJson<dto::Settings>,
) -> Result<Json<dto::Settings>, Failure> {
    caller.require(settings_need(&s.machine.settings(), &body))?;
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

#[cfg(test)]
mod tests {
    use super::attachment;

    #[test]
    fn download_names_keep_any_letters_and_stay_valid_headers() {
        let h = attachment("kørsel \"1\".fbcap");
        assert_eq!(
            h,
            "attachment; filename=\"k_rsel _1_.fbcap\"; filename*=UTF-8''k%C3%B8rsel%20%221%22.fbcap"
        );
        assert!(axum::http::HeaderValue::from_str(&h).is_ok());
    }
}
