use axum::{extract::State, Json};
use serde::Deserialize;
use uuid::Uuid;

use crate::ai_client::{Chunk, ConceptMeta, ExerciseTemplate};
use crate::auth::middleware::AuthUser;
use crate::state::AppState;

use super::errors::ExerciseError;
use super::service;

#[derive(Deserialize)]
pub struct GetOrGenerateRequest {
    concept_id: Uuid,
    concept_meta: ConceptMeta,
    #[serde(default)]
    top_chunks: Vec<Chunk>,
    #[serde(default)]
    batch_children: Vec<Uuid>,
}

/// No automatic trigger exists yet (needs journey/DAG creation — same
/// gap as markdown/deferred.md #4/#26) — exposed as a real, directly
/// callable endpoint so the race-prevention mechanism itself is
/// independently testable now, not gated on that other work existing
/// first. Requires auth (any signed-in user) but has no ownership
/// check — canonical exercises are global content, not user-scoped.
pub async fn get_or_generate(
    AuthUser(_user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<GetOrGenerateRequest>,
) -> Result<Json<Vec<ExerciseTemplate>>, ExerciseError> {
    let templates = service::get_or_generate(
        &state,
        req.concept_id,
        req.concept_meta,
        req.top_chunks,
        req.batch_children,
    )
    .await?;
    Ok(Json(templates))
}
