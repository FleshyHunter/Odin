use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{begin_rls_transaction, AuthUser};
use crate::journeys::turn::create_journey_thread_sync;
use crate::state::AppState;

use super::errors::TracksError;

// Track = study_threads (models/track.rs's own comment) — subject_title/
// current_concept_title/status aren't columns on study_threads itself,
// joined in from journeys/subjects/canonical_concepts every time. Plain
// snake_case field names on the wire, not camelCase — matches every
// other module's response DTO (api/journeys.ts does the camelCase
// translation frontend-side, same convention this follows).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TrackResponse {
    thread_id: Uuid,
    title: String,
    subject_title: String,
    current_concept_title: Option<String>,
    status: String,
    is_pinned: bool,
    project_id: Option<Uuid>,
    last_active_at: DateTime<Utc>,
    journey_id: Uuid,
}

// sqlx 0.9's SqlSafeStr bound requires &'static str literals (injection
// audit) — no runtime format!()/query-fragment composition, so the
// shared JOIN shape below is just duplicated across the two literal
// queries that need it, matching every other handler in this codebase
// (none of which build queries dynamically either).

/// GET /tracks — every non-deleted journey-mode thread this user owns.
/// journeys.deleted_at is deliberately NOT filtered here — a Track whose
/// underlying journey was independently soft-deleted still shows
/// normally (no cascade either direction, per explicit requirement);
/// the join still resolves fine since a soft-deleted journey row (and
/// its subject/title data) is never actually removed.
pub async fn list_tracks(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<TrackResponse>>, TracksError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let tracks: Vec<TrackResponse> = sqlx::query_as(
        "SELECT st.thread_id, COALESCE(st.title, s.title) AS title, s.title AS subject_title, \
         cc.title AS current_concept_title, j.status AS status, st.is_pinned, st.project_id, \
         st.last_active_at, st.journey_id \
         FROM study_threads st \
         JOIN journeys j ON j.journey_id = st.journey_id \
         JOIN subjects s ON s.subject_id = j.subject_id \
         LEFT JOIN canonical_concepts cc ON cc.concept_id = st.current_concept_id \
         WHERE st.mode = 'journey' AND st.deleted_at IS NULL AND st.user_id = $1 \
         ORDER BY st.last_active_at DESC",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(tracks))
}

async fn fetch_track(pool: &sqlx::PgPool, user_id: Uuid, thread_id: Uuid) -> Result<TrackResponse, TracksError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let track: Option<TrackResponse> = sqlx::query_as(
        "SELECT st.thread_id, COALESCE(st.title, s.title) AS title, s.title AS subject_title, \
         cc.title AS current_concept_title, j.status AS status, st.is_pinned, st.project_id, \
         st.last_active_at, st.journey_id \
         FROM study_threads st \
         JOIN journeys j ON j.journey_id = st.journey_id \
         JOIN subjects s ON s.subject_id = j.subject_id \
         LEFT JOIN canonical_concepts cc ON cc.concept_id = st.current_concept_id \
         WHERE st.mode = 'journey' AND st.deleted_at IS NULL AND st.thread_id = $1",
    )
    .bind(thread_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    track.ok_or(TracksError::NotFound)
}

#[derive(Deserialize)]
pub struct CreateTrackRequest {
    title: String,
    journey_id: Uuid,
}

/// POST /tracks — creates the real study_threads row AND generates/
/// stores the tutor's opening message in the same call (see
/// create_journey_thread_sync's own doc comment for why this has to be
/// atomic rather than lazy). Slower than a plain insert (a real
/// generation call), same tradeoff already accepted for the Onboarding
/// Diagnostic step earlier in the same wizard this is called from.
pub async fn create_track(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateTrackRequest>,
) -> Result<Json<TrackResponse>, TracksError> {
    if req.title.trim().is_empty() {
        return Err(TracksError::Validation("title must not be empty".to_string()));
    }
    let (thread_id, _concept_title) =
        create_journey_thread_sync(&state, user_id, req.journey_id, req.title.trim()).await?;
    Ok(Json(fetch_track(&state.pool, user_id, thread_id).await?))
}

/// DELETE /tracks/{id} — soft delete only (SCHEMA.md's own comment on
/// study_threads.deleted_at: "hides, never hard-deletes — audit_logs/
/// messages/quiz_attempts referencing this thread must survive").
/// RLS scopes the UPDATE to this user's own rows; 0 rows affected means
/// either it doesn't exist or isn't this user's — both surface as the
/// same 404, same IDOR-safe convention as everywhere else (Rule 34).
pub async fn delete_track(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, TracksError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query("UPDATE study_threads SET deleted_at = NOW() WHERE thread_id = $1 AND deleted_at IS NULL")
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(TracksError::NotFound);
    }
    tx.commit().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn touch_and_fetch(
    state: &AppState,
    user_id: Uuid,
    thread_id: Uuid,
    sql: &'static str,
) -> Result<Json<TrackResponse>, TracksError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query(sql).bind(thread_id).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(TracksError::NotFound);
    }
    tx.commit().await?;
    Ok(Json(fetch_track(&state.pool, user_id, thread_id).await?))
}

/// POST /tracks/{id}/pin — toggles, matching TrackMenu's own single
/// "Pin"/"Unpin" menu item (no separate pin/unpin endpoints needed).
pub async fn pin_track(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
) -> Result<Json<TrackResponse>, TracksError> {
    touch_and_fetch(
        &state,
        user_id,
        thread_id,
        "UPDATE study_threads SET is_pinned = NOT is_pinned WHERE thread_id = $1 AND deleted_at IS NULL",
    )
    .await
}

#[derive(Deserialize)]
pub struct RenameTrackRequest {
    title: String,
}

pub async fn rename_track(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
    Json(req): Json<RenameTrackRequest>,
) -> Result<Json<TrackResponse>, TracksError> {
    if req.title.trim().is_empty() {
        return Err(TracksError::Validation("title must not be empty".to_string()));
    }
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query("UPDATE study_threads SET title = $1 WHERE thread_id = $2 AND deleted_at IS NULL")
        .bind(req.title.trim())
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(TracksError::NotFound);
    }
    tx.commit().await?;
    Ok(Json(fetch_track(&state.pool, user_id, thread_id).await?))
}

#[derive(Deserialize)]
pub struct SetTrackProjectRequest {
    // Same operation for both TrackMenu's "Change project" (Some) and
    // "Remove from project" (None) — matches api/tracks.ts's own mock
    // comment (deferred.md #41).
    project_id: Option<Uuid>,
}

pub async fn set_track_project(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
    Json(req): Json<SetTrackProjectRequest>,
) -> Result<Json<TrackResponse>, TracksError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query("UPDATE study_threads SET project_id = $1 WHERE thread_id = $2 AND deleted_at IS NULL")
        .bind(req.project_id)
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(TracksError::NotFound);
    }
    tx.commit().await?;
    Ok(Json(fetch_track(&state.pool, user_id, thread_id).await?))
}
