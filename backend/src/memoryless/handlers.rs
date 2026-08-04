use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::middleware::{begin_rls_transaction, AuthUser};
use crate::state::AppState;

use super::errors::MemorylessError;
use super::staging::{self, load_owned, StagedThread};
use super::turn;

#[derive(Deserialize)]
pub struct SendMessageRequest {
    // Absent on the first message of a brand new thread — Rule 11: a
    // thread exists only once the first message is sent, and (per the
    // Memoryless Mode section) Rust generates its UUID itself upfront,
    // never Postgres via DEFAULT gen_random_uuid().
    thread_id: Option<Uuid>,
    message: String,
    // User-controlled, per markdown/deferred.md #20 point 1 — defaults
    // to `true` (today's behavior) when omitted, NOT auto-decided by
    // detected_intent.
    think: Option<bool>,
}

/// Streams the turn back as Server-Sent Events rather than one blocking
/// JSON response (Block 11 follow-up — markdown/deferred.md #20 point 2):
///   event: thread  — the thread_id, sent once immediately (the only way
///                     a caller learns a brand-new thread's ID, since it
///                     doesn't exist until this call creates it)
///   event: delta   — one per text chunk, as qwen produces them
///   event: done    — sent once persistence finishes, successful or not
///                     (the turn's own outcome — cancelled/stalled/error
///                     — already lives in the staged audit event, not in
///                     this final marker)
pub async fn send_message(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, MemorylessError> {
    let thread = match req.thread_id {
        Some(thread_id) => load_owned(&state, thread_id, user_id).await?,
        None => StagedThread::new(Uuid::new_v4(), user_id),
    };
    let thread_id = thread.thread_id;

    if req.message.trim().is_empty() {
        return Err(MemorylessError::Validation("message must not be empty".to_string()));
    }
    let think = req.think.unwrap_or(true);

    let mut rx = turn::start_turn_stream(&state, thread, req.message, think).await?;

    let sse_stream = async_stream::stream! {
        yield Ok(Event::default().event("thread").data(thread_id.to_string()));
        while let Some(event) = rx.recv().await {
            // deferred.md #53: a real, distinguishable "error" event —
            // previously a mid-generation failure just ended the stream
            // with zero deltas, indistinguishable from an empty-but-
            // successful reply. `done` still follows unconditionally
            // either way, same as before — it marks "stream over," not
            // "stream succeeded."
            match event {
                turn::TurnEvent::Delta(text) => yield Ok(Event::default().event("delta").data(text)),
                turn::TurnEvent::Error(reason) => yield Ok(Event::default().event("error").data(reason)),
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

pub async fn get_thread(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
) -> Result<Json<StagedThread>, MemorylessError> {
    let thread = load_owned(&state, thread_id, user_id).await?;
    Ok(Json(thread))
}

#[derive(Serialize)]
pub struct ConvertResponse {
    thread_id: Uuid,
    converted: bool,
}

/// The "checkout" moment (PRD.md, Memoryless Mode: CONVERSION) —
/// SCOPED DOWN for this pass to messages + audit_logs only. Does NOT
/// create a journey: onboarding-diagnostic orchestration and journey
/// creation depend on Milestone 10 work not yet built (see
/// markdown/deferred.md #4) — the resulting study_threads row simply
/// stays journey_id NULL, mode='memoryless', now durable in Postgres
/// instead of Redis-staged. Commit-then-delete ordering (Block 11 spec
/// point 4): the Redis key is only removed AFTER the Postgres commit
/// succeeds, so a failed write never loses staged data.
pub async fn convert(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
) -> Result<Json<ConvertResponse>, MemorylessError> {
    let thread = load_owned(&state, thread_id, user_id).await?;

    // Extend the sliding window the instant conversion begins (spec
    // point 5) — a cheap mitigation for the race between this load and
    // the Postgres commit finishing.
    staging::refresh_ttl(&state, thread_id).await?;

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    sqlx::query(
        "INSERT INTO study_threads (thread_id, user_id, mode, created_at, last_active_at) \
         VALUES ($1, $2, 'memoryless', $3, NOW())",
    )
    .bind(thread.thread_id)
    .bind(thread.user_id)
    .bind(thread.created_at)
    .execute(&mut *tx)
    .await?;

    for message in &thread.messages {
        sqlx::query(
            "INSERT INTO messages (thread_id, role, content, mode, timestamp) \
             VALUES ($1, $2, $3, 'memoryless', $4)",
        )
        .bind(thread.thread_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.timestamp)
        .execute(&mut *tx)
        .await?;
    }

    for event in &thread.audit_events {
        let matched_concepts = json!(event.matched_concepts);
        sqlx::query(
            "INSERT INTO audit_logs \
             (thread_id, user_input, cleaned_query, matched_concepts, detected_intent, \
              mode, response_text, model_used, error, timestamp) \
             VALUES ($1, $2, $3, $4, $5, 'memoryless', $6, $7, $8, $9)",
        )
        .bind(thread.thread_id)
        .bind(&event.user_input)
        .bind(&event.cleaned_query)
        .bind(matched_concepts)
        .bind(&event.detected_intent)
        .bind(&event.response_text)
        .bind(&event.model_used)
        .bind(&event.error)
        .bind(event.timestamp)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // thread.staged_uploads is deliberately NOT committed here (Block
    // 12 — markdown/deferred.md #17/#25/#27): material_upload has no
    // real ChromaDB destination yet, and prompt_upload can't pass RLS
    // at all without a real journey_id to attach to (neither exists —
    // both are Milestone 9/10 dependencies). Stated plainly, not
    // softened: any staged_uploads on this thread are LOST at this
    // point, not merely delayed — they live in the SAME Redis key
    // being deleted below, so a successfully deduped/chunked/embedded
    // upload does not survive its thread's conversion in this pass. A
    // real gap, tracked rather than hidden.

    // Only remove the Redis entry once Postgres durably has it.
    staging::delete(&state, thread_id).await?;

    Ok(Json(ConvertResponse { thread_id: thread.thread_id, converted: true }))
}
