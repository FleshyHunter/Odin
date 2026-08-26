use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::middleware::{begin_rls_transaction, AuthUser};
use crate::auth::rate_limit;
use crate::journeys;
use crate::state::AppState;

use super::errors::MemorylessError;
use super::staging::{self, load_owned, StagedAuditEvent, StagedMessage, StagedThread};
use super::turn;
use super::write_through;

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
    // deferred.md #78: a real generate_stream() call against Ollama —
    // checked before even loading/creating the staged thread, same
    // "reject cheaply, before any real work" placement as the auth
    // endpoints.
    let mut conn = state.get_redis_connection().await?;
    let (max, window) = state.memoryless_message_rate_limit;
    if !rate_limit::check_and_increment(&mut conn, &rate_limit::memoryless_message_key(user_id), max, window).await? {
        return Err(MemorylessError::RateLimited);
    }

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

// deferred.md #70 — a separate wire-facing shape, not #[serde(skip)] on
// StagedChunk itself: that same struct is also what gets serialized to
// Redis (staging::save), and skip_serializing there would silently
// drop the embedding from STORAGE too, breaking #19's real
// rank_by_similarity() retrieval — this DTO only affects what THIS one
// HTTP response ships, nothing persisted.
#[derive(Serialize)]
pub struct ThreadResponseChunk {
    pub text: String,
    pub token_count: i32,
}

#[derive(Serialize)]
pub struct ThreadResponseUpload {
    pub content_hash: String,
    pub filename: String,
    pub upload_role: String,
    pub extracted_text: String,
    pub chunks: Vec<ThreadResponseChunk>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ThreadResponse {
    pub thread_id: Uuid,
    pub user_id: Uuid,
    pub messages: Vec<StagedMessage>,
    pub audit_events: Vec<StagedAuditEvent>,
    pub staged_uploads: Vec<ThreadResponseUpload>,
    pub created_at: DateTime<Utc>,
}

impl From<StagedThread> for ThreadResponse {
    fn from(thread: StagedThread) -> Self {
        Self {
            thread_id: thread.thread_id,
            user_id: thread.user_id,
            messages: thread.messages,
            audit_events: thread.audit_events,
            staged_uploads: thread
                .staged_uploads
                .into_iter()
                .map(|u| ThreadResponseUpload {
                    content_hash: u.content_hash,
                    filename: u.filename,
                    upload_role: u.upload_role,
                    extracted_text: u.extracted_text,
                    chunks: u
                        .chunks
                        .into_iter()
                        .map(|c| ThreadResponseChunk { text: c.text, token_count: c.token_count })
                        .collect(),
                    created_at: u.created_at,
                })
                .collect(),
            created_at: thread.created_at,
        }
    }
}

pub async fn get_thread(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
) -> Result<Json<ThreadResponse>, MemorylessError> {
    let thread = load_owned(&state, thread_id, user_id).await?;
    Ok(Json(thread.into()))
}

#[derive(Deserialize)]
pub struct ConvertRequest {
    // Milestone 10 (deferred.md #4/#40) is done — a real journey must
    // already exist before conversion runs (created through the
    // existing TrackModal flow). Conversion just links it; it does not
    // run the onboarding diagnostic itself, which would be redundant —
    // the student already proved engagement through this conversation.
    journey_id: Uuid,
}

#[derive(Serialize)]
pub struct ConvertResponse {
    thread_id: Uuid,
    converted: bool,
}

/// The "checkout" moment (PRD.md, Memoryless Mode: CONVERSION) —
/// deferred.md #17: now genuinely links a real journey (Milestone 10 —
/// #4/#40 — exists), instead of leaving `study_threads.journey_id` NULL
/// forever. Commit-then-delete ordering (Block 11 spec point 4): the
/// Redis key is only removed AFTER every Postgres write below succeeds
/// (including the prompt_upload commit at the very end), so a failure
/// anywhere in this function never loses staged data.
///
/// deferred.md #56: write-through may already have committed some (or
/// all) of this thread's messages/audit_logs/study_threads
/// incrementally, per turn, well before this is ever called — this is
/// no longer the only path to durability, just the one remaining real
/// "checkout" moment (attaching a journey). `study_threads` conflicts
/// safely on its own primary key. `messages`/`audit_logs` have no
/// natural business key to conflict on instead, so this counts what's
/// already there for this thread_id and only inserts whatever
/// write-through hasn't caught up on yet — safe, not approximate, since
/// write-through and this load both process `thread.messages`/
/// `thread.audit_events` in the exact same append order every time.
/// Those historical rows keep `mode = 'memoryless'` — an honest record
/// of what mode they were actually sent in; only new messages sent
/// afterward, through the real `/journeys/{id}/messages` (#2a), get
/// `mode = 'journey'`.
pub async fn convert(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(thread_id): Path<Uuid>,
    Json(req): Json<ConvertRequest>,
) -> Result<Json<ConvertResponse>, MemorylessError> {
    let thread = load_owned(&state, thread_id, user_id).await?;

    // Extend the sliding window the instant conversion begins (spec
    // point 5) — a cheap mitigation for the race between this load and
    // the Postgres commit finishing.
    staging::refresh_ttl(&state, thread_id).await?;

    // RLS-scoped ownership check (Rule 34) — reuses #2a's own
    // entry-concept resolution rather than duplicating that join.
    let subject_id = journeys::turn::verify_journey_and_subject(&state.pool, user_id, req.journey_id).await?;
    let dag_version: i32 = sqlx::query_scalar("SELECT dag_version FROM subjects WHERE subject_id = $1")
        .bind(subject_id)
        .fetch_one(&state.pool)
        .await?;
    let entry =
        journeys::turn::fetch_entry_concept(&state.pool, user_id, req.journey_id, subject_id, dag_version).await?;

    // deferred.md #87: study_threads.journey_id has no UNIQUE constraint,
    // and the INSERT below only conflicts on thread_id (this thread's own
    // primary key) — without this check, a DIFFERENT memoryless thread
    // converting to a journey_id another thread already owns would create
    // two real mode='journey' rows sharing that journey_id, silently.
    // Same guard journeys::turn::start_journey_thread already applies to
    // its own creation path, reused here rather than duplicated. Only
    // blocks a genuinely different thread — re-converting this exact
    // thread_id (idempotent retry) is unaffected, same as the ON CONFLICT
    // below already tolerates.
    if let Some(existing_thread_id) = journeys::turn::find_thread(&state.pool, user_id, req.journey_id).await?
        && existing_thread_id != thread.thread_id
    {
        return Err(MemorylessError::Validation(
            "this journey already has a teaching thread".to_string(),
        ));
    }

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    sqlx::query(
        "INSERT INTO study_threads (thread_id, user_id, journey_id, mode, current_concept_id, created_at, last_active_at) \
         VALUES ($1, $2, $3, 'journey', $4, $5, NOW()) \
         ON CONFLICT (thread_id) DO UPDATE SET \
           journey_id = EXCLUDED.journey_id, mode = 'journey', \
           current_concept_id = EXCLUDED.current_concept_id, last_active_at = NOW()",
    )
    .bind(thread.thread_id)
    .bind(thread.user_id)
    .bind(req.journey_id)
    .bind(entry.concept_id)
    .bind(thread.created_at)
    .execute(&mut *tx)
    .await?;

    // Flow 4's own "teach from first concept" — this thread's existing
    // conversation now counts as that concept actively being taught,
    // same as #2a's opening turn marks it for a brand-new thread.
    sqlx::query("UPDATE journey_concepts SET status = 'in_progress' WHERE journey_id = $1 AND concept_id = $2")
        .bind(req.journey_id)
        .bind(entry.concept_id)
        .execute(&mut *tx)
        .await?;

    let existing_message_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE thread_id = $1")
        .bind(thread.thread_id)
        .fetch_one(&mut *tx)
        .await?;
    for message in thread.messages.iter().skip(existing_message_count as usize) {
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

    let existing_audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE thread_id = $1")
        .bind(thread.thread_id)
        .fetch_one(&mut *tx)
        .await?;
    for event in thread.audit_events.iter().skip(existing_audit_count as usize) {
        let matched_concepts = json!(event.matched_concepts);
        // deferred.md #60: queued_ms/generation_ms, real instrumentation now.
        sqlx::query(
            "INSERT INTO audit_logs \
             (thread_id, user_input, cleaned_query, matched_concepts, detected_intent, \
              mode, response_text, model_used, error, queued_ms, generation_ms, timestamp) \
             VALUES ($1, $2, $3, $4, $5, 'memoryless', $6, $7, $8, $9, $10, $11)",
        )
        .bind(thread.thread_id)
        .bind(&event.user_input)
        .bind(&event.cleaned_query)
        .bind(matched_concepts)
        .bind(&event.detected_intent)
        .bind(&event.response_text)
        .bind(&event.model_used)
        .bind(&event.error)
        .bind(event.queued_ms)
        .bind(event.generation_ms)
        .bind(event.timestamp)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // deferred.md #27, closing for real now: a real journey_id exists
    // (required by `sources`' own CHECK constraint for this role), so
    // any prompt_upload staged earlier in this same conversation can
    // finally be committed. material_upload staged uploads are NOT
    // re-committed here — deferred.md #56 already write-through commits
    // those at upload time, independent of this moment. Propagated with
    // `?`, not logged-and-swallowed: a failure here must abort before
    // the Redis delete below, or this data would be genuinely lost, not
    // merely delayed (same "commit-then-delete" guarantee as the rest
    // of this function).
    for upload in thread.staged_uploads.iter().filter(|u| u.upload_role == "prompt_upload") {
        write_through::write_through_prompt_upload(&state, user_id, req.journey_id, subject_id, upload).await?;
    }

    // Only remove the Redis entry once Postgres durably has everything.
    staging::delete(&state, thread_id).await?;

    Ok(Json(ConvertResponse { thread_id: thread.thread_id, converted: true }))
}
