// Rule 32/51, SCHEMA.md's content_flags table (deferred.md #33) — full
// schema + RLS existed since Block 2, but no Rust endpoint ever called
// it. Not blocked on anything: POST /exercises/get_or_generate already
// creates real, flaggable exercises rows today.

mod errors;
mod handlers;

use axum::{routing::post, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/content_flags", post(handlers::create_flag))
}
