// Block 12: uploads (markdown/deferred.md #21-25, #27). Reuses
// memoryless::errors::MemorylessError and memoryless::staging directly
// rather than a parallel error/staging type — a staged upload lives
// inside the SAME Redis-staged thread blob memoryless mode already
// owns, not a separate store, so the identical thread-ownership rules
// (410 expired, 404 not-yours) apply here unchanged.

mod dedup;
mod handlers;

use axum::{routing::post, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/uploads", post(handlers::upload))
}
