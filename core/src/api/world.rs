//! The simulated world's routes (docs/api.md, "The simulated world"): the prebuilt worlds, the
//! current one, switching it and putting the machine back at its start. The `world` WebSocket
//! topic is in `ws`.

use super::{ApiJson, AppState, Failure, blocking, convert, dto};
use crate::sim::worlds::WorldInfo;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/v0/worlds", get(worlds))
        .route("/api/v0/world", get(world).put(set_world))
        .route("/api/v0/world/reset", post(reset))
}

async fn worlds(State(s): State<AppState>) -> Json<Vec<WorldInfo>> {
    Json(s.machine.worlds())
}

async fn world(State(s): State<AppState>) -> Result<Json<dto::WorldView>, Failure> {
    let view = blocking(move || s.machine.world()).await?;
    Ok(Json(convert::world_view(view)))
}

async fn set_world(
    State(s): State<AppState>,
    ApiJson(body): ApiJson<dto::WorldRequest>,
) -> Result<Json<dto::WorldView>, Failure> {
    let view = blocking(move || s.machine.set_world(&body.name, body.start)).await??;
    Ok(Json(convert::world_view(view)))
}

async fn reset(State(s): State<AppState>) -> Result<StatusCode, Failure> {
    blocking(move || s.machine.reset_world()).await?;
    Ok(StatusCode::NO_CONTENT)
}
