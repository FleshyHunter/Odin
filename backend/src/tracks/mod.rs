// Real backend for the Track resource (Track = study_threads,
// models/track.rs's own comment) — api/tracks.ts was fully mocked until
// now, in-memory only, lost on every refresh. Every handler here matches
// a "Real contract" comment already written in that mock file.
//
// study_threads.deleted_at has existed since migration 0001 but nothing
// ever read or wrote it before this module — RLS does NOT filter it
// automatically (checked: study_threads_isolation only checks user_id),
// so every query here adds `AND deleted_at IS NULL` by hand.

mod errors;
mod handlers;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tracks", get(handlers::list_tracks).post(handlers::create_track))
        .route("/tracks/{id}", axum::routing::delete(handlers::delete_track))
        .route("/tracks/{id}/pin", post(handlers::pin_track))
        .route("/tracks/{id}/rename", post(handlers::rename_track))
        .route("/tracks/{id}/project", post(handlers::set_track_project))
}
