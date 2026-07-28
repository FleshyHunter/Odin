// Block 11: Memoryless Mode's Redis-staged chat turns (PRD.md,
// Memoryless Mode). Everything here stages in Redis, never Postgres,
// until an explicit conversion — see staging.rs/handlers.rs::convert.

pub mod errors;
mod handlers;
// Exercised only by its own unit tests for now (Block 11 spec point 6)
// — no staged-upload embedding pipeline exists yet to call it for real,
// same as ai_client's own not-yet-wired functions (see main.rs).
#[allow(dead_code)]
mod similarity;
mod staging;
mod turn;

use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/memoryless/messages", post(handlers::send_message))
        .route(
            "/memoryless/threads/{thread_id}",
            get(handlers::get_thread),
        )
        .route(
            "/memoryless/threads/{thread_id}/convert",
            post(handlers::convert),
        )
}
