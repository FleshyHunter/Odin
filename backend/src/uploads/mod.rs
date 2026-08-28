// Block 12: uploads (markdown/deferred.md #21-25, #27). Reuses
// memoryless::errors::MemorylessError and memoryless::staging directly
// rather than a parallel error/staging type — a staged upload lives
// inside the SAME Redis-staged thread blob memoryless mode already
// owns, not a separate store, so the identical thread-ownership rules
// (410 expired, 404 not-yours) apply here unchanged.

mod dedup;
// deferred.md #92: pub(crate), not private — memoryless::turn now calls
// stage_one_file()/StagedFileOutcome directly (bundled multi-file Send),
// not just this module's own /uploads route.
pub(crate) mod handlers;

use axum::extract::DefaultBodyLimit;
use axum::{routing::post, Router};

use crate::state::AppState;

// Axum's own Multipart default body limit is 2MB — far below PRD.md's
// locked MEMORYLESS_STAGED_UPLOAD_MAX_MB default (25MB), found live
// while testing the size guardrail (deferred.md #32): a file between
// 2MB and the real configured max was silently rejected by axum itself
// with a generic "could not read file" error, never even reaching the
// guardrail's actual check or its real rejection message. Raised here,
// scoped to just this route (not a blanket global increase for every
// endpoint), to comfortably clear the real configured max plus a
// generous margin for multipart's own encoding overhead (boundaries,
// headers, the role/thread_id text fields) — so a legitimately-
// oversized upload hits OUR check and OUR message, not axum's generic
// one, and axum's limit is purely a hard backstop above that.
const MULTIPART_OVERHEAD_BUFFER_BYTES: usize = 1024 * 1024;

pub fn router(max_upload_mb: u64) -> Router<AppState> {
    let body_limit = (max_upload_mb as usize) * 1024 * 1024 + MULTIPART_OVERHEAD_BUFFER_BYTES;
    Router::new()
        .route("/uploads", post(handlers::upload))
        .layer(DefaultBodyLimit::max(body_limit))
}
