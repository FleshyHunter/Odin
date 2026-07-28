// Redis-staged memoryless thread (PRD.md, Memoryless Mode: "nothing
// memoryless writes to Postgres... everything stages in Redis first").
// key: memoryless:{thread_id}, value: this whole struct as JSON.
//
// Sliding TTL (Block 11 spec, negotiated): every `save` call rewrites
// the key with a FRESH TTL rather than merely touching an existing one
// — the natural behavior of a SET-with-expiry on every message, no
// separate "touch" step needed for the common case.

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::errors::MemorylessError;
use crate::state::AppState;

fn thread_key(thread_id: Uuid) -> String {
    format!("memoryless:{thread_id}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedMessage {
    // "user" | "tutor" — matches messages.role's CHECK constraint
    // (migrations/0001_initial_schema.sql) so conversion can insert
    // these rows verbatim, no translation step.
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

// Mirrors the subset of audit_logs' columns (models/audit.rs) that a
// bare, foundation-less memoryless turn can actually populate — no
// retrieved_chunk_ids/strategy_used/assessment_result/queued_ms/
// generation_ms, since retrieval and grading don't run in this pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedAuditEvent {
    pub user_input: String,
    pub cleaned_query: String,
    pub matched_concepts: Vec<String>,
    pub detected_intent: String,
    pub response_text: String,
    pub model_used: String,
    // Reuses audit_logs' existing `error` column (models/audit.rs) —
    // None for a turn that completed normally; Some(reason) for one cut
    // short (client disconnect, a stalled-too-long gap, or an
    // ai_service-side failure) — see turn.rs::start_turn_stream and
    // markdown/deferred.md #20. response_text still holds whatever
    // partial text was generated before the cutoff (industrial
    // Claude/ChatGPT behavior — never silently discarded).
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

// Block 12 (markdown/deferred.md #21/#23): a chunk of an ingestion-tier
// upload (prompt_upload/material_upload only — ephemeral is never
// staged/chunked/embedded at all, per its own definition), already
// extracted+chunked+embedded by ai_service's POST /ingest. Mirrors the
// `chunks` table's shape closely enough that conversion can insert
// these near-verbatim once a real destination (Postgres now, ChromaDB
// once #25 exists) is available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedChunk {
    pub text: String,
    pub token_count: i32,
    pub embedding: Vec<f32>,
}

// One staged, deduped upload. `content_hash` is stored so a LATER
// upload in the same session can be checked against this one too
// (Duplicate Upload Prevention's third scope: "this session's own
// Redis-staged uploads") without re-deriving it from raw bytes again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedUpload {
    pub content_hash: String,
    pub filename: String,
    // "prompt_upload" | "material_upload" — matches sources.upload_role's
    // CHECK constraint (migrations/0001_initial_schema.sql).
    pub upload_role: String,
    pub extracted_text: String,
    pub chunks: Vec<StagedChunk>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedThread {
    pub thread_id: Uuid,
    pub user_id: Uuid,
    pub messages: Vec<StagedMessage>,
    pub audit_events: Vec<StagedAuditEvent>,
    pub staged_uploads: Vec<StagedUpload>,
    pub created_at: DateTime<Utc>,
}

impl StagedThread {
    pub fn new(thread_id: Uuid, user_id: Uuid) -> Self {
        Self {
            thread_id,
            user_id,
            messages: Vec::new(),
            audit_events: Vec::new(),
            staged_uploads: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

/// Loads a staged thread if it exists and hasn't expired. Returns
/// `None` for both "never existed" and "expired" — Rule 11 treats those
/// identically (no record persists in Redis OR Postgres for an
/// abandoned interaction either way), so there is nothing meaningful to
/// distinguish; callers surface both as `ThreadExpiredOrNotFound`.
pub async fn load(state: &AppState, thread_id: Uuid) -> Result<Option<StagedThread>, MemorylessError> {
    let mut conn = state.get_redis_connection().await?;
    let raw: Option<String> = conn.get(thread_key(thread_id)).await?;
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// Writes the thread back with a FRESH TTL (SET, not a plain touch) —
/// this IS the sliding-window mechanic: every save (every new message)
/// resets the clock to the full window.
pub async fn save(state: &AppState, thread: &StagedThread) -> Result<(), MemorylessError> {
    let mut conn = state.get_redis_connection().await?;
    let json = serde_json::to_string(thread)?;
    let ttl_seconds = (state.memoryless_thread_ttl_minutes * 60).max(1) as u64;
    conn.set_ex::<_, _, ()>(thread_key(thread.thread_id), json, ttl_seconds).await?;
    Ok(())
}

/// Refreshes the TTL alone, without touching the value — called at the
/// very start of conversion (Block 11 spec point 5) to mitigate the
/// race where the sliding window could lapse mid-conversion, between
/// the initial `load` and the Postgres commit finishing.
pub async fn refresh_ttl(state: &AppState, thread_id: Uuid) -> Result<(), MemorylessError> {
    let mut conn = state.get_redis_connection().await?;
    let ttl_seconds = (state.memoryless_thread_ttl_minutes * 60).max(1);
    conn.expire::<_, ()>(thread_key(thread_id), ttl_seconds).await?;
    Ok(())
}

/// Deletes the staged entry — called only AFTER the Postgres commit at
/// conversion succeeds (Block 11 spec point 4: commit-then-delete, so a
/// failed write never loses staged data).
pub async fn delete(state: &AppState, thread_id: Uuid) -> Result<(), MemorylessError> {
    let mut conn = state.get_redis_connection().await?;
    conn.del::<_, ()>(thread_key(thread_id)).await?;
    Ok(())
}

/// Loads a staged thread and enforces ownership (Rule 34): `None` from
/// Redis is a clean 410 (expired or never existed — Rule 11 makes no
/// distinction meaningful), a thread owned by someone else is a 404
/// (never confirm existence to a non-owner). Shared by both
/// memoryless::handlers (chat turns) and uploads::handlers (Block 12) —
/// both need the identical "load this thread_id, only if it's really
/// mine" check.
pub async fn load_owned(state: &AppState, thread_id: Uuid, user_id: Uuid) -> Result<StagedThread, MemorylessError> {
    let thread = load(state, thread_id)
        .await?
        .ok_or(MemorylessError::ThreadExpiredOrNotFound)?;
    if thread.user_id != user_id {
        return Err(MemorylessError::NotFound);
    }
    Ok(thread)
}
