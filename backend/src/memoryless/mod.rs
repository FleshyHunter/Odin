// Block 11: Memoryless Mode's Redis-staged chat turns (PRD.md,
// Memoryless Mode). Everything here stages in Redis, never Postgres,
// until an explicit conversion — see staging.rs/handlers.rs::convert.
// deferred.md #56 (write_through.rs) adds a SECOND, independent path
// to durability alongside that explicit conversion — see its own doc
// comment for the split between what that covers and what it doesn't.

pub mod errors;
mod handlers;
// pub: uploads:: (Block 12) shares StagedThread/StagedUpload and
// load_owned/save with memoryless::handlers — a staged upload lives
// inside the SAME Redis-staged thread blob, not a separate store.
pub mod staging;
mod turn;
// pub: uploads:: (Block 12) also calls write_through_material_upload
// directly, right after staging a material_upload — same "shared
// across the memoryless/uploads boundary" reasoning as staging above.
pub mod write_through;

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
