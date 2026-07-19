use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    database: &'static str,
}

// Debugging hygiene only (Health Endpoints, locked) — no failover, no
// complex monitoring. Pings the pool with a trivial query so this
// actually confirms the DB connection Block 1 sets up, not just that
// the HTTP server itself is up.
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => (
            StatusCode::OK,
            Json(HealthResponse { status: "ok", database: "connected" }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse { status: "degraded", database: "unreachable" }),
        ),
    }
}
