//! Layer 4: HTTP + WebSocket (docs/api.md). Translates between the wire and the machine; no
//! logic of its own. Machine calls that may block (starting or stopping parts) run on the
//! blocking pool so they never stall the async runtime.

mod convert;
pub mod dto;
mod files;
mod shell;
mod ws;

use crate::logs::LogBook;
use crate::machine::{ErrorKind, Machine, MachineError, NewPart};
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
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
        .route("/api/v0/connectors", get(connectors))
        .route("/api/v0/ports", get(ports))
        .route("/api/v0/ports/problems", get(port_problems))
        .route("/api/v0/parts", get(parts).post(add_part))
        .route("/api/v0/parts/{name}", delete(remove_part))
        .route("/api/v0/ws", get(ws::upgrade))
        .merge(files::routes())
        .fallback(shell::serve)
        .with_state(state)
}

/// Every failed request answers with an `ApiError` body: what happened and what to try.
pub struct Failure(StatusCode, dto::ErrorBody);

impl Failure {
    fn new(status: StatusCode, code: &str, message: impl Into<String>, hint: impl Into<String>) -> Failure {
        Failure(status, dto::ErrorBody { code: code.into(), message: message.into(), hint: hint.into() })
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
        config_problem: s.machine.config_problem().map(|e| dto::ErrorBody {
            code: "config-broken".into(),
            message: match e.line {
                Some(l) => format!("parts.toml has a problem on line {l}: {}", e.message),
                None => format!("parts.toml has a problem: {}", e.message),
            },
            hint: e.hint,
        }),
    })
}

async fn connectors(State(s): State<AppState>) -> Json<Vec<dto::ConnectorInfo>> {
    Json(s.machine.connectors().iter().map(convert::connector).collect())
}

async fn ports(State(s): State<AppState>) -> Json<Vec<dto::PortInfo>> {
    Json(s.machine.ports().iter().map(convert::port).collect())
}

async fn port_problems(State(s): State<AppState>) -> Json<Vec<dto::PortProblem>> {
    Json(s.machine.port_problems().iter().map(convert::port_problem).collect())
}

async fn parts(State(s): State<AppState>) -> Json<Vec<dto::PartInfo>> {
    Json(s.machine.parts().iter().map(convert::part).collect())
}

async fn add_part(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::NewPart>,
) -> Result<(StatusCode, Json<dto::PartInfo>), Failure> {
    let req = NewPart {
        name: body.name,
        connector: body.connector,
        port: body.port,
        mount: body.mount.unwrap_or_default(),
    };
    let view = blocking(move || s.machine.add_part(req)).await??;
    Ok((StatusCode::CREATED, Json(convert::part(&view))))
}

async fn remove_part(State(s): State<AppState>, Path(name): Path<String>) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.remove_part(&name)).await??;
    Ok(StatusCode::NO_CONTENT)
}
