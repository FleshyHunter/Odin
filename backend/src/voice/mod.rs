// deferred.md #80 — the real backend half of Voice Input. Deliberately
// its own small module rather than folded into memoryless/ or uploads/:
// this is a bare AI-gateway passthrough (audio in, text out), reachable
// from EITHER chat mode's Composer, with no thread/journey scoping and
// no Postgres/Redis persistence at all — it doesn't belong to either
// mode. `session` is the one exception: an in-memory-only, ephemeral
// registry for chunked-streaming transcription (see its own doc
// comment) — still nothing durable, torn down per-recording.

pub mod errors;
mod handlers;
pub mod session;

use axum::extract::DefaultBodyLimit;
use axum::{routing::post, Router};

use crate::state::AppState;

// Axum's own Multipart default body limit is 2MB (same gap uploads::
// mod.rs already documents and fixed for its own route) — this route
// had no override at all until now, so any voice recording whose
// encoded size crossed 2MB was silently rejected by axum itself before
// ever reaching our own validation. A fixed const, not a config knob:
// this is headroom for a multi-minute webm/opus recording, not
// something an operator would ever need to tune per-deployment.
const VOICE_MAX_UPLOAD_MB: u64 = 10;
const MULTIPART_OVERHEAD_BUFFER_BYTES: usize = 1024 * 1024;

pub fn router() -> Router<AppState> {
    let body_limit = (VOICE_MAX_UPLOAD_MB as usize) * 1024 * 1024 + MULTIPART_OVERHEAD_BUFFER_BYTES;
    Router::new()
        .route("/voice/transcribe", post(handlers::transcribe))
        .route("/voice/transcribe/stream", post(handlers::stream_start))
        .route("/voice/transcribe/chunk", post(handlers::stream_chunk))
        .layer(DefaultBodyLimit::max(body_limit))
}
