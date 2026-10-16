//! Layer 4: HTTP + WebSocket (docs/api.md). Translates between the wire and the machine; no
//! logic of its own. Machine calls that may block (starting or stopping parts) run on the
//! blocking pool so they never stall the async runtime.

mod apps;
mod convert;
mod driving;
pub mod dto;
mod files;
mod flash;
mod packages;
mod shell;
mod translations;
mod world;
mod ws;

use crate::logs::LogBook;
use crate::machine::{ErrorKind, Machine, MachineError, NewPart};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub machine: Arc<Machine>,
    pub logs: Arc<LogBook>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v0/system", get(system))
        .route("/api/v0/about", get(about))
        .route("/api/v0/connectors", get(connectors))
        .route("/api/v0/connectors/problems", get(connector_problems))
        .route("/api/v0/ports", get(ports))
        .route("/api/v0/ports/problems", get(port_problems))
        .route("/api/v0/ports/check-board", post(check_board))
        .route("/api/v0/ports/scan-i2c", post(scan_i2c))
        .route("/api/v0/ports/header", get(header))
        .route("/api/v0/parts", get(parts).post(add_part))
        .route("/api/v0/parts/{name}", delete(remove_part))
        .route("/api/v0/signals/{part}/{name}", get(signal_value))
        .route("/api/v0/ws", get(ws::upgrade))
        .merge(files::routes())
        .merge(packages::routes())
        .merge(apps::routes())
        .merge(driving::routes())
        .merge(flash::routes())
        .merge(translations::routes())
        .merge(world::routes())
        .fallback(shell::serve)
        .with_state(state)
}

/// Every failed request answers with an `ApiError` body: what happened and what to try.
pub struct Failure(StatusCode, dto::ErrorBody);

impl Failure {
    fn new(status: StatusCode, code: &str, message: impl Into<String>, hint: impl Into<String>) -> Failure {
        Failure(
            status,
            dto::ErrorBody { code: code.into(), message: message.into(), hint: hint.into(), reason: None },
        )
    }
}

impl From<MachineError> for Failure {
    fn from(e: MachineError) -> Failure {
        let status = match e.kind {
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::Unprocessable => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::Locked => StatusCode::LOCKED,
            ErrorKind::InsufficientStorage => StatusCode::INSUFFICIENT_STORAGE,
            ErrorKind::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        };
        Failure(status, convert::error_body(&e))
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, Json(dto::ApiError { error: self.1 })).into_response()
    }
}

/// JSON bodies whose mistakes come back as an `ApiError` too, not as plain text.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: serde::de::DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = Failure;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(rejection) => Err(json_failure(rejection)),
        }
    }
}

fn json_failure(rejection: JsonRejection) -> Failure {
    Failure::new(
        StatusCode::BAD_REQUEST,
        "bad-request",
        format!("The request body is not what this endpoint expects: {}", rejection.body_text()),
        "Check the body against docs/api.md.",
    )
}

/// Runs a machine call on the blocking pool.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, Failure> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        Failure::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("The core failed while handling this request: {e}"),
            "This is a bug in Faber. The log has details; please report it.",
        )
    })
}

async fn system(State(s): State<AppState>) -> Json<dto::SystemInfo> {
    Json(dto::SystemInfo {
        name: "Faber".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        api: env!("FABER_API").into(),
        time_us: s.machine.clock().now_us(),
        time_zone: s.machine.time_zone(),
        config_problem: s.machine.config_problem().map(|e| dto::ErrorBody {
            code: "config-broken".into(),
            message: match e.line {
                Some(l) => format!("parts.toml has a problem on line {l}: {}", e.message),
                None => format!("parts.toml has a problem: {}", e.message),
            },
            hint: e.hint,
            reason: None,
        }),
        buses: match s.machine.bus_support() {
            crate::ports::BusSupport::System => dto::Buses::System,
            crate::ports::BusSupport::Fake => dto::Buses::Fake,
            crate::ports::BusSupport::None => dto::Buses::None,
        },
    })
}

async fn about(State(s): State<AppState>) -> Result<Json<dto::AboutInfo>, Failure> {
    let about = blocking(move || s.machine.about()).await?;
    Ok(Json(dto::AboutInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        api: env!("FABER_API").into(),
        data_folder: about.data_dir.display().to_string(),
        uptime_s: about.uptime_us / 1_000_000,
        os: about.os,
        time_zone: about.time_zone,
    }))
}

async fn connectors(State(s): State<AppState>) -> Json<Vec<dto::ConnectorInfo>> {
    Json(s.machine.connector_kinds().iter().map(|(kind, m)| convert::connector(m, *kind)).collect())
}

async fn connector_problems(State(s): State<AppState>) -> Json<Vec<dto::ConnectorProblem>> {
    Json(s.machine.connector_problems().iter().map(convert::connector_problem).collect())
}

async fn ports(State(s): State<AppState>) -> Json<Vec<dto::PortInfo>> {
    Json(s.machine.ports().iter().map(convert::port).collect())
}

async fn port_problems(State(s): State<AppState>) -> Json<Vec<dto::PortProblem>> {
    Json(s.machine.port_problems().iter().map(convert::port_problem).collect())
}

/// Asks a free port whether a FaberLink board is on it. Takes up to a few seconds.
async fn check_board(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::BoardCheckRequest>,
) -> Result<Json<dto::BoardCheck>, Failure> {
    let check = blocking(move || s.machine.check_for_board(&body.port)).await??;
    Ok(Json(convert::board_check(&check)))
}

/// A safe I²C scan of one bus, only when asked (docs/api.md, "I²C scan").
async fn scan_i2c(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::I2cScanRequest>,
) -> Result<Json<dto::I2cScan>, Failure> {
    let scan = blocking(move || s.machine.scan_i2c(&body.port)).await??;
    Ok(Json(convert::i2c_scan(&scan)))
}

/// The Pi's 40-pin header: the same on every Pi with one, whatever this machine is.
async fn header() -> Json<Vec<dto::HeaderPin>> {
    Json(
        crate::ports::header::HEADER
            .iter()
            .map(|p| dto::HeaderPin {
                pin: p.pin,
                name: p.name.into(),
                line: p.line,
                function: (!p.function.is_empty()).then(|| p.function.into()),
            })
            .collect(),
    )
}

/// The newest reading of one signal. The WebSocket streams them; this is for simple programs.
async fn signal_value(
    State(s): State<AppState>,
    Path((part, name)): Path<(String, String)>,
) -> Result<Json<dto::SignalValue>, Failure> {
    let raw = format!("{part}/{name}");
    let unknown = || {
        Failure::new(
            StatusCode::NOT_FOUND,
            "unknown-signal",
            format!("There is no signal called \"{raw}\" right now."),
            "GET /api/v0/parts lists every part's signals; a stopped or removed part has none.",
        )
    };
    let address = crate::names::SignalAddress::parse(&raw).ok_or_else(unknown)?;
    let router = s.machine.router();
    let capability =
        router.signals().into_iter().find(|(a, _)| *a == address).map(|(_, c)| c).ok_or_else(unknown)?;
    let reading = router.latest(&address).ok_or_else(unknown)?;
    Ok(Json(dto::SignalValue { address: raw, capability, reading: reading.map(|r| (*r).clone()) }))
}

async fn parts(State(s): State<AppState>) -> Json<Vec<dto::PartInfo>> {
    Json(s.machine.parts().iter().map(convert::part).collect())
}

async fn add_part(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::NewPart>,
) -> Result<(StatusCode, Json<dto::PartInfo>), Failure> {
    let settings = convert::settings(body.settings.unwrap_or_default()).map_err(|e| {
        Failure::new(
            StatusCode::BAD_REQUEST,
            "invalid-settings",
            format!("The settings can't be saved in parts.toml: {e}"),
            "Use numbers, text, true/false and lists; leave out null.",
        )
    })?;
    let req = NewPart {
        name: body.name,
        connector: body.connector,
        port: body.port,
        address: body.address,
        simulate: body.simulate.map(convert::simulate_from),
        mount: body.mount.unwrap_or_default(),
        settings,
    };
    let view = blocking(move || s.machine.add_part(req)).await??;
    Ok((StatusCode::CREATED, Json(convert::part(&view))))
}

async fn remove_part(State(s): State<AppState>, Path(name): Path<String>) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.remove_part(&name)).await??;
    Ok(StatusCode::NO_CONTENT)
}
