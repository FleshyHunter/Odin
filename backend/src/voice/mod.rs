// deferred.md #80 — the real backend half of Voice Input. Deliberately
// its own small module rather than folded into memoryless/ or uploads/:
// this is a bare AI-gateway passthrough (audio in, text out), reachable
// from EITHER chat mode's Composer, with no thread/journey scoping and
// no persistence at all — it doesn't belong to either mode.

pub mod errors;
mod handlers;

use axum::{routing::post, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/voice/transcribe", post(handlers::transcribe))
}
