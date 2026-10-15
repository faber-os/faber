//! Driving routes (docs/api.md, "Driving"): leases, commands, manual simulation values and the
//! e-stop. Each one hands straight to the machine; the rules live there.

use super::{ApiJson, AppState, Failure, blocking, convert, dto};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use faber_capabilities::estop::Source;

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/leases", get(leases))
        .route("/api/v0/parts/{name}/lease", post(request_lease).delete(release_lease))
        .route("/api/v0/parts/{name}/lease/renew", post(renew_lease))
        .route("/api/v0/parts/{name}/command", post(command))
        .route("/api/v0/parts/{name}/simulation", get(simulation).put(set_simulation))
        .route("/api/v0/estop", get(estop).post(engage_estop))
        .route("/api/v0/estop/release", post(release_estop))
}

async fn leases(State(s): State<AppState>) -> Json<Vec<dto::LeaseInfo>> {
    Json(s.machine.leases().iter().map(convert::lease_info).collect())
}

async fn request_lease(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::LeaseRequest>,
) -> Result<(StatusCode, Json<dto::LeaseGrant>), Failure> {
    let lease = blocking(move || s.machine.request_lease(&name, &body.holder, body.take_over)).await??;
    Ok((StatusCode::CREATED, Json(convert::lease_grant(&lease))))
}

async fn renew_lease(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::LeaseRenewal>,
) -> Result<Json<dto::LeaseGrant>, Failure> {
    let lease = blocking(move || s.machine.renew_lease(&name, &body.lease)).await??;
    Ok(Json(convert::lease_grant(&lease)))
}

#[derive(serde::Deserialize)]
struct LeaseQuery {
    #[serde(default)]
    lease: String,
}

async fn release_lease(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Query(q): Query<LeaseQuery>,
) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.release_lease(&name, &q.lease)).await??;
    Ok(StatusCode::NO_CONTENT)
}

async fn command(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::CommandRequest>,
) -> Result<Json<dto::CommandResult>, Failure> {
    let reply = blocking(move || s.machine.command(&name, &body.lease, body.command)).await??;
    Ok(Json(dto::CommandResult {
        applied: reply.applied,
        clamped: reply.clamped,
        deadline_us: reply.deadline_us,
    }))
}

async fn simulation(
    State(s): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<dto::Simulation>, Failure> {
    let pattern = blocking(move || s.machine.pattern(&name)).await??;
    Ok(Json(dto::Simulation { pattern: convert::pattern(pattern) }))
}

async fn set_simulation(
    State(s): State<AppState>,
    Path(name): Path<String>,
    ApiJson(body): ApiJson<dto::Simulation>,
) -> Result<Json<dto::Simulation>, Failure> {
    let pattern = convert::pattern_from(body.pattern);
    let pattern = blocking(move || s.machine.set_pattern(&name, pattern)).await??;
    Ok(Json(dto::Simulation { pattern: convert::pattern(pattern) }))
}

async fn estop(State(s): State<AppState>) -> Json<dto::EstopStatus> {
    Json(convert::estop_status(&s.machine.estop()))
}

/// Always allowed, for anyone: an empty body is fine, and even a body the core can't read still
/// engages it (with a reason that says so), because a stop must never fail on a typo.
async fn engage_estop(State(s): State<AppState>, body: Bytes) -> Result<Json<dto::EstopStatus>, Failure> {
    let (source, reason) = match serde_json::from_slice::<dto::EstopRequest>(&body) {
        Ok(b) => (b.source.unwrap_or(Source::App), b.reason.unwrap_or_default()),
        Err(_) if body.iter().all(u8::is_ascii_whitespace) => (Source::App, String::new()),
        Err(e) => (Source::App, format!("(the request body could not be read: {e})")),
    };
    let view = blocking(move || s.machine.engage_estop(source, &reason)).await?;
    Ok(Json(convert::estop_status(&view)))
}

async fn release_estop(State(s): State<AppState>, body: Bytes) -> Result<Json<dto::EstopStatus>, Failure> {
    let source = if body.iter().all(u8::is_ascii_whitespace) {
        Source::App
    } else {
        let b: dto::EstopReleaseRequest = serde_json::from_slice(&body).map_err(|e| {
            Failure::new(
                StatusCode::BAD_REQUEST,
                "bad-request",
                format!("The request body is not what this endpoint expects: {e}"),
                "Send {} or {\"source\": \"shell\"}.",
            )
        })?;
        b.source.unwrap_or(Source::App)
    };
    let view = blocking(move || s.machine.release_estop(source)).await??;
    Ok(Json(convert::estop_status(&view)))
}
