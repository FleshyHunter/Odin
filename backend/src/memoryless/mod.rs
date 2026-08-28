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
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};

use crate::state::AppState;

// deferred.md #92: same "axum's own Multipart default (2MB) is far below
// a real configured max" gap uploads::mod.rs's own MULTIPART_OVERHEAD_
// BUFFER_BYTES already fixed for /uploads — /memoryless/messages is now
// multipart too (up to handlers::MAX_FILES_PER_SEND files bundled into
// one turn), so it needs the same treatment, sized for the whole batch
// rather than one file. Scoped to just this one route (chained onto its
// own MethodRouter before it's handed to .route()), not a blanket
// increase across every memoryless endpoint — the other two routes here
// stay on axum's own default.
const MULTIPART_OVERHEAD_BUFFER_BYTES: usize = 1024 * 1024;

pub fn router(max_upload_mb: u64) -> Router<AppState> {
    let body_limit =
        (max_upload_mb as usize) * (handlers::MAX_FILES_PER_SEND) * 1024 * 1024 + MULTIPART_OVERHEAD_BUFFER_BYTES;
    Router::new()
        .route(
            "/memoryless/messages",
            post(handlers::send_message).layer(DefaultBodyLimit::max(body_limit)),
        )
        .route(
            "/memoryless/threads/{thread_id}",
            get(handlers::get_thread),
        )
        .route(
            "/memoryless/threads/{thread_id}/convert",
            post(handlers::convert),
        )
}
