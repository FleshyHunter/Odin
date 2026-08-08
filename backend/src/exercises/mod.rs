// Block 12: exercise-template race prevention (markdown/deferred.md
// #3, #26) — a standalone module, deliberately separate from uploads/
// despite both being "Block 12" in the sequencing doc; conceptually
// unrelated to files, so it gets its own home rather than being
// crammed into an uploads-named module.

pub mod errors;
mod handlers;
// deferred.md #4: journeys::service reuses get_or_generate/fetch_one_canonical
// directly for an existing subject's diagnostic exercise — a diagnostic IS
// just another canonical exercise template, not a separate concept.
pub(crate) mod service;

use axum::{routing::post, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/exercises/get_or_generate", post(handlers::get_or_generate))
}
