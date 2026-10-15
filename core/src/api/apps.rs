//! App routes (docs/api.md, "Apps"): list, turn on and off, pick the parts for the roles, and
//! the apps' UI files. Installing and uninstalling an app go through `/api/v0/packages`.

use super::{ApiJson, AppState, Failure, blocking, convert, dto};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use std::collections::BTreeMap;

/// What an app's UI may do: load its own files, and nothing on the network. It talks to the
/// shell only through `postMessage` (docs/apps.md §4), never to the API directly.
///
/// `sandbox allow-scripts` makes every UI page sandboxed, with an opaque origin, even when it is
/// opened on its own outside the shell's iframe: it never runs as the shell's origin (which could
/// reach the API). It may not frame anything (`frame-src 'none'`), so it can't put the shell or
/// another page inside itself.
pub const UI_CSP: &str = "default-src 'self'; connect-src 'none'; script-src 'self'; \
    style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self'; \
    object-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'self'; base-uri 'none'; \
    form-action 'none'; frame-ancestors 'self'; sandbox allow-scripts";

/// An app's own icon is shown as an image only. Opened on its own, it is a sandboxed document that
/// may run nothing and load nothing.
pub const ICON_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox";

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/apps", get(list))
        .route("/api/v0/apps/{id}", get(one))
        .route("/api/v0/apps/{id}/enable", post(enable))
        .route("/api/v0/apps/{id}/disable", post(disable))
        .route("/api/v0/apps/{id}/roles", put(roles))
        .route("/apps/{id}/ui/{*path}", get(ui_file))
        .route("/apps/{id}/icon.svg", get(icon))
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<dto::AppInfo>>, Failure> {
    let apps = blocking(move || s.machine.apps()).await?;
    Ok(Json(apps.iter().map(convert::app_info).collect()))
}

async fn one(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<dto::AppInfo>, Failure> {
    let app = blocking(move || s.machine.app(&id)).await??;
    Ok(Json(convert::app_info(&app)))
}

async fn enable(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<dto::AppInfo>, Failure> {
    let app = blocking(move || s.machine.set_app_enabled(&id, true)).await??;
    Ok(Json(convert::app_info(&app)))
}

async fn disable(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<dto::AppInfo>, Failure> {
    let app = blocking(move || s.machine.set_app_enabled(&id, false)).await??;
    Ok(Json(convert::app_info(&app)))
}

async fn roles(
    State(s): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<dto::AppRolesRequest>,
) -> Result<Json<dto::AppInfo>, Failure> {
    let wanted: BTreeMap<String, Vec<String>> = body
        .roles
        .into_iter()
        .map(|(role, parts)| {
            let parts = match parts {
                dto::RoleParts::One(p) => vec![p],
                dto::RoleParts::Many(ps) => ps,
            };
            (role, parts)
        })
        .collect();
    let app = blocking(move || s.machine.set_app_roles(&id, wanted)).await??;
    Ok(Json(convert::app_info(&app)))
}

/// One file of an app's UI, with a strict content security policy and no type sniffing.
async fn ui_file(
    State(s): State<AppState>,
    Path((id, path)): Path<(String, String)>,
) -> Result<Response, Failure> {
    let (bytes, kind) = blocking(move || s.machine.app_ui_file(&id, &path)).await??;
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(UI_CSP));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    // The UI runs with an opaque origin (`null`), so loading its own module scripts and fonts is
    // a cross-origin request: allowed for exactly that origin. Nothing here is secret (it is the
    // app's package), and the API itself never answers cross-origin requests.
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("null"));
    Ok(res)
}

/// The icon at the root of an app's package, as an image only.
async fn icon(State(s): State<AppState>, Path(id): Path<String>) -> Result<Response, Failure> {
    let bytes = blocking(move || s.machine.app_icon(&id)).await??;
    let mut res = bytes.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/svg+xml"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(ICON_CSP));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(res)
}
