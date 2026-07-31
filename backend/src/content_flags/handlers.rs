use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{begin_rls_transaction, AuthUser};
use crate::state::AppState;

use super::errors::ContentFlagError;

#[derive(Deserialize)]
pub struct CreateFlagRequest {
    source_id: Option<Uuid>,
    chunk_id: Option<Uuid>,
    exercise_id: Option<Uuid>,
    reason: String,
}

#[derive(Serialize)]
pub struct FlagResponse {
    flag_id: Uuid,
}

/// POST /content_flags — Rule 32's manual "flag as wrong" action, the
/// SOLE mechanism for correcting bad/conflicting shared content (no
/// automated contradiction detector — embedding similarity can't
/// reliably distinguish agreement from disagreement on the same
/// claim). content_flags is RLS-protected (SCHEMA.md) — a flag is
/// visible only to the user who created it. Resolving a flag
/// (resolved_at) is a manual, human-in-the-loop process outside this
/// app (no review/moderation UI is in scope, per Rule 32's own
/// framing) — this endpoint only ever creates a flag.
pub async fn create_flag(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateFlagRequest>,
) -> Result<Json<FlagResponse>, ContentFlagError> {
    // Matches content_flags' own CHECK constraint (SCHEMA.md) — exactly
    // one of the three references, never zero or more than one.
    let provided_count = [req.source_id.is_some(), req.chunk_id.is_some(), req.exercise_id.is_some()]
        .into_iter()
        .filter(|&present| present)
        .count();
    if provided_count != 1 {
        return Err(ContentFlagError::Validation(
            "exactly one of source_id, chunk_id, or exercise_id must be provided".to_string(),
        ));
    }
    if req.reason.trim().is_empty() {
        return Err(ContentFlagError::Validation("reason must not be empty".to_string()));
    }

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    // deferred.md #44: sources/chunks are RLS mixed-scope (a private
    // prompt_upload row must never be confirmed to exist for a non-
    // owner) — exercises has no RLS at all (canonical, globally
    // visible), so only these two need an explicit pre-check here.
    // Relying on the INSERT's own FK constraint for existence would
    // bypass RLS entirely (Postgres FK checks always do, regardless of
    // policy), turning "flag as wrong" into a cross-account existence
    // oracle for private content. This SELECT runs inside the same
    // RLS-scoped transaction as the insert below, so it sees exactly
    // what this caller is allowed to see — nothing more.
    if let Some(source_id) = req.source_id {
        let visible: Option<(Uuid,)> = sqlx::query_as("SELECT source_id FROM sources WHERE source_id = $1")
            .bind(source_id)
            .fetch_optional(&mut *tx)
            .await?;
        if visible.is_none() {
            return Err(ContentFlagError::NotFound);
        }
    }
    if let Some(chunk_id) = req.chunk_id {
        let visible: Option<(Uuid,)> = sqlx::query_as("SELECT chunk_id FROM chunks WHERE chunk_id = $1")
            .bind(chunk_id)
            .fetch_optional(&mut *tx)
            .await?;
        if visible.is_none() {
            return Err(ContentFlagError::NotFound);
        }
    }

    let flag_id: Uuid = sqlx::query_scalar(
        "INSERT INTO content_flags (user_id, source_id, chunk_id, exercise_id, reason)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING flag_id",
    )
    .bind(user_id)
    .bind(req.source_id)
    .bind(req.chunk_id)
    .bind(req.exercise_id)
    .bind(&req.reason)
    .fetch_one(&mut *tx)
    .await
    // A bogus/nonexistent id trips content_flags' own FK constraints
    // (source_id/chunk_id/exercise_id all REFERENCE their table) — a
    // clean 400 is more honest than the generic 500 the blanket
    // From<sqlx::Error> below would otherwise produce, same pattern
    // already used for AuthError::AccountExists' unique-violation catch.
    .map_err(|err| match &err {
        sqlx::Error::Database(db_err) if db_err.is_foreign_key_violation() => {
            ContentFlagError::Validation("referenced content does not exist".to_string())
        }
        _ => ContentFlagError::from(err),
    })?;
    tx.commit().await?;

    Ok(Json(FlagResponse { flag_id }))
}
