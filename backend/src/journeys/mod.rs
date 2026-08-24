// Onboarding Diagnostic orchestration (deferred.md #4, #6, #13; PRD.md's
// Onboarding Diagnostic, Steps 1-4) — the one remaining foundation
// blocking most of the rest of the backlog (deferred_sequence.md):
// nothing before this ever created a real journeys/journey_concepts
// row. #40's intake UI (`TrackModal`/`api/journeys.ts`) now calls this
// for real, end to end — the mocked `createTrackFromJourney` only wraps
// the resulting real `journey_id` into this app's still-partially-mocked
// local Track model, not a stand-in for this module itself.

pub mod errors;
mod handlers;
mod instantiate;
mod service;
pub mod staging;
// pub(crate): memoryless::handlers::convert (deferred.md #17) reuses
// verify_journey_and_subject/fetch_entry_concept directly — the same
// entry-concept resolution #2a's own /start already needs.
pub(crate) mod turn;

use axum::{routing::{get, post}, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/journeys/start", post(handlers::start))
        // Soft delete, independent of study_threads/Track deletion —
        // no cascade either direction, explicit requirement.
        .route("/journeys/{journey_id}", axum::routing::delete(handlers::delete_journey))
        .route("/journeys/diagnostic/{diagnostic_id}/respond", post(handlers::respond))
        .route(
            "/journeys/diagnostic/{diagnostic_id}/confirm-downgrade",
            post(handlers::confirm_downgrade),
        )
        .route(
            "/journeys/diagnostic/{diagnostic_id}/retry-backup",
            post(handlers::retry_backup),
        )
        // deferred.md #2a — the real journey-mode chat turn loop.
        .route("/journeys/{journey_id}/start", post(handlers::start_journey_thread))
        .route(
            "/journeys/{journey_id}/messages",
            post(handlers::send_journey_message).get(handlers::get_journey_messages),
        )
        // deferred.md — the exercise loop: serve a fresh instantiated
        // question, then grade a submitted answer against it.
        .route(
            "/journeys/{journey_id}/concepts/{concept_id}/exercise",
            post(handlers::serve_exercise),
        )
        .route(
            "/journeys/{journey_id}/exercises/{attempt_id}/submit",
            post(handlers::submit_exercise_answer),
        )
        // #1/#2: mastery_bank/quiz_attempts had zero readers before this.
        .route(
            "/journeys/{journey_id}/concepts/{concept_id}/mastery",
            get(handlers::get_mastery_status),
        )
        .route(
            "/journeys/{journey_id}/concepts/{concept_id}/history",
            get(handlers::get_node_history),
        )
}
