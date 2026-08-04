// Block 11: Memoryless Mode's Redis-staged chat turns (PRD.md,
// Memoryless Mode). Everything here stages in Redis, never Postgres,
// until an explicit conversion — see staging.rs/handlers.rs::convert.

pub mod errors;
mod handlers;
// deferred.md #19: wired into turn.rs's staged-upload retrieval —
// no longer unwired/dead code.
mod similarity;
// pub: uploads:: (Block 12) shares StagedThread/StagedUpload and
// load_owned/save with memoryless::handlers — a staged upload lives
// inside the SAME Redis-staged thread blob, not a separate store.
pub mod staging;
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
