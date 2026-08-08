// Onboarding Diagnostic orchestration (deferred.md #4, #6, #13; PRD.md's
// Onboarding Diagnostic, Steps 1-4) — the one remaining foundation
// blocking most of the rest of the backlog (deferred_sequence.md):
// nothing before this ever created a real journeys/journey_concepts
// row. Frontend stays on the existing mocked createTrack() this pass
// (deferred.md #40's intake UI collects real fields but doesn't call
// this yet) — this module is built and verified standalone.

pub mod errors;
mod handlers;
mod instantiate;
mod service;
pub mod staging;

use axum::{routing::post, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/journeys/start", post(handlers::start))
        .route("/journeys/diagnostic/{diagnostic_id}/respond", post(handlers::respond))
        .route(
            "/journeys/diagnostic/{diagnostic_id}/confirm-downgrade",
            post(handlers::confirm_downgrade),
        )
        .route(
            "/journeys/diagnostic/{diagnostic_id}/retry-backup",
            post(handlers::retry_backup),
        )
}
