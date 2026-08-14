// deferred.md #56: incremental Postgres write-through for a memoryless
// thread's own content — Redis stops being the SOLE record of a
// still-memoryless conversation, without waiting for an explicit
// `convert()` (which was already scoped to exactly this same
// messages/audit_logs/study_threads data, just done once in bulk at
// conversion instead of incrementally per turn). A thread that never
// gets explicitly converted no longer just vanishes when its Redis key
// expires — the TTL still fires exactly as before (deferred.md #55
// unchanged), it just no longer means "data destroyed," only "this
// live session ended."
//
// Originally scoped EXACTLY to the two-thirds deferred.md #56's own
// feasibility check found clean at the time: messages/study_threads/
// audit_logs (this file's write_through_turn), and material_upload
// staged uploads (this file's write_through_material_upload) —
// deliberately NOT prompt_upload, since `sources`' own CHECK constraint
// requires a real journey_id for that role, which didn't exist yet
// (Milestone 10). Callers filter upload_role themselves before calling
// write_through_material_upload; this module trusts that and does not
// re-check it.
//
// deferred.md #17: Milestone 10 now exists, so write_through_prompt_upload
// (below) closes that remaining gap — but only callable once a real
// journey_id is known, which is exactly memoryless::handlers::convert's
// own moment, not upload time. Unlike its material_upload sibling, this
// one is NOT called from uploads::handlers::upload — a prompt_upload
// genuinely has nowhere valid to land until conversion.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::staging::{StagedAuditEvent, StagedMessage, StagedUpload};
use crate::ai_client::{self, ChunkRecord};
use crate::auth::middleware::begin_rls_transaction;
use crate::memoryless::errors::MemorylessError;
use crate::state::AppState;

// PRD.md, Trust Score Default Policy: "user_upload (anything a person
// uploads themselves, unverified) -> 0.35" — the one source_type every
// staged upload uses (uploads::handlers only ever sets source_type =
// 'user_upload'), so this is the one real default this module needs.
const USER_UPLOAD_TRUST_SCORE: f32 = 0.35;

/// Ensures a durable `study_threads` row exists for this thread. The
/// FIRST write-through call for a given thread creates it — mode is
/// always 'memoryless' here, journey_id always NULL (no journey exists
/// yet to set, same scope `convert()` already committed to). Every
/// later call for the same thread_id is a harmless no-op via
/// `ON CONFLICT`, since `thread_id` is `study_threads`' own primary key.
async fn ensure_study_thread(
    pool: &PgPool,
    user_id: Uuid,
    thread_id: Uuid,
    thread_created_at: DateTime<Utc>,
) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    sqlx::query(
        "INSERT INTO study_threads (thread_id, user_id, mode, created_at, last_active_at) \
         VALUES ($1, $2, 'memoryless', $3, NOW()) \
         ON CONFLICT (thread_id) DO UPDATE SET last_active_at = NOW()",
    )
    .bind(thread_id)
    .bind(user_id)
    .bind(thread_created_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Writes THIS turn's own new messages + audit event through to
/// Postgres, immediately — called once per completed turn
/// (`memoryless/turn.rs`), right alongside the existing Redis
/// `staging::save`. Never re-processes an earlier turn's rows: the
/// caller passes exactly the rows THIS turn just created (fresh
/// `StagedMessage`/`StagedAuditEvent` values, not yet pushed anywhere
/// else), so there is no dedup/"have I already written this" state to
/// track — each call is independent and self-contained.
pub async fn write_through_turn(
    pool: &PgPool,
    user_id: Uuid,
    thread_id: Uuid,
    thread_created_at: DateTime<Utc>,
    messages: &[StagedMessage],
    audit_event: &StagedAuditEvent,
) -> Result<(), MemorylessError> {
    ensure_study_thread(pool, user_id, thread_id, thread_created_at).await?;

    let mut tx = begin_rls_transaction(pool, user_id).await?;
    for message in messages {
        sqlx::query(
            "INSERT INTO messages (thread_id, role, content, mode, timestamp) \
             VALUES ($1, $2, $3, 'memoryless', $4)",
        )
        .bind(thread_id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.timestamp)
        .execute(&mut *tx)
        .await?;
    }

    let matched_concepts = serde_json::json!(audit_event.matched_concepts);
    sqlx::query(
        "INSERT INTO audit_logs \
         (thread_id, user_input, cleaned_query, matched_concepts, detected_intent, \
          mode, response_text, model_used, error, timestamp) \
         VALUES ($1, $2, $3, $4, $5, 'memoryless', $6, $7, $8, $9)",
    )
    .bind(thread_id)
    .bind(&audit_event.user_input)
    .bind(&audit_event.cleaned_query)
    .bind(matched_concepts)
    .bind(&audit_event.detected_intent)
    .bind(&audit_event.response_text)
    .bind(&audit_event.model_used)
    .bind(&audit_event.error)
    .bind(audit_event.timestamp)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Writes one `material_upload` staged upload through to Postgres,
/// immediately — called once, right after it's staged
/// (`uploads::handlers::upload`). `ON CONFLICT` targets the SAME
/// partial unique index `find_committed`'s dedup check already relies
/// on (`sources_content_hash_global_unique`) — a real TOCTOU race
/// against that earlier, separate-transaction dedup check is
/// theoretically possible (not yet actually observed), and this makes
/// a duplicate insert a harmless no-op rather than a hard failure.
///
/// deferred.md #18b: also the one real call site for the shared
/// `knowledge_global` write — only `material_upload` writes there
/// (`prompt_upload` stays journey-private by design, `sources_mixed_scope`'s
/// whole point). Runs AFTER the Postgres commit, fail-soft (a warning,
/// not a propagated error): the upload already succeeded from the
/// student's point of view once the transaction below commits: a
/// failure writing to the shared KB, or backfilling `chroma_id`, only
/// means this content isn't retrievable via knowledge_global yet, not
/// that the upload itself failed. `concept_id` is deliberately left
/// NULL for every chunk — `assign_concept()` has zero real data in
/// `concept_embeddings` anywhere yet (explicit, discussed scope cut,
/// not an oversight); PRD's own "misc chunk — still retrievable, just
/// unassigned" is an explicitly valid state.
///
/// `origin_journey_id` (2026-08-13, migration 0006): purely an origin
/// tag — WHICH journey this upload happened during, if any — never a
/// visibility scope. Deliberately its OWN column, not the existing
/// `journey_id` (which stays `prompt_upload`'s real privacy scope,
/// CHECK-constrained to `NULL` for `material_upload` — confirmed via
/// the actual constraint, not assumed; overloading it would make one
/// column mean two different things depending on `upload_role`, same
/// trap this codebase already avoided once for `subjects.
/// entry_concept_id`, migration 0004). `sources_mixed_scope`'s RLS
/// policy for `material_upload` is `upload_role = 'material_upload'`
/// unconditionally — it never consults either journey column — so
/// setting this has zero effect on staying globally visible/searchable
/// everywhere. Exists so `journeys::turn::fetch_journey_upload_context`
/// can always-include "what was uploaded during THIS journey"
/// regardless of role. `None` for memoryless-mode uploads (no journey
/// context to tag).
pub async fn write_through_material_upload(
    state: &AppState,
    user_id: Uuid,
    upload: &StagedUpload,
    origin_journey_id: Option<Uuid>,
) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    // The ON CONFLICT predicate below must match
    // sources_content_hash_global_unique's own WHERE clause EXACTLY
    // (migrations/0001_initial_schema.sql) — Postgres requires a
    // literal syntactic match to a partial unique index's predicate to
    // infer it as the conflict target, not just a logically-equivalent
    // one. Confirmed live: omitting "AND content_hash IS NOT NULL"
    // here (implied but not written) raised a real 42P10 "no unique or
    // exclusion constraint matching the ON CONFLICT specification."
    let source_id: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO sources \
         (filename, source_type, upload_role, upload_scope, trust_score, content_hash, retrieval_date, origin_journey_id) \
         VALUES ($1, 'user_upload', 'material_upload', 'global', $2, $3, $4, $5) \
         ON CONFLICT (content_hash) WHERE upload_role = 'material_upload' AND content_hash IS NOT NULL \
         DO NOTHING \
         RETURNING source_id",
    )
    .bind(&upload.filename)
    .bind(USER_UPLOAD_TRUST_SCORE)
    .bind(&upload.content_hash)
    .bind(upload.created_at)
    .bind(origin_journey_id)
    .fetch_optional(&mut *tx)
    .await?;

    // (chunk_id, embedding) for every row this call actually inserted —
    // captured via RETURNING so the knowledge_global write below can use
    // Postgres's own chunk_id as ChromaDB's document id exactly
    // (ARCHITECTURE.md's locked schema: "id: chunk_id (matches
    // PostgreSQL chunks.chunk_id)").
    let mut inserted_chunks: Vec<(Uuid, Vec<f32>)> = Vec::new();

    // A genuine conflict (no row returned) means another insert already
    // owns this content_hash — nothing left to chunk against here.
    let source_id_for_metadata = source_id.map(|(id,)| id);
    if let Some(source_id) = source_id_for_metadata {
        for chunk in &upload.chunks {
            let chunk_id: Uuid = sqlx::query_scalar(
                "INSERT INTO chunks (source_id, text, token_count) VALUES ($1, $2, $3) RETURNING chunk_id",
            )
            .bind(source_id)
            .bind(&chunk.text)
            .bind(chunk.token_count)
            .fetch_one(&mut *tx)
            .await?;
            inserted_chunks.push((chunk_id, chunk.embedding.clone()));
        }
    }

    tx.commit().await?;

    if let (Some(source_id), false) = (source_id_for_metadata, inserted_chunks.is_empty()) {
        write_chunks_to_knowledge_global(state, user_id, source_id, &inserted_chunks, "material_upload", None, None)
            .await;
    }

    Ok(())
}

/// deferred.md #18b's fail-soft shared-KB write, split out of
/// write_through_material_upload so that function's own control flow
/// stays readable. Never returns an error — every failure (ai_service
/// unreachable, the backfill UPDATE itself) is logged and swallowed, by
/// design (see the caller's own doc comment on why fail-soft is correct
/// here).
///
/// deferred.md #37: generalized beyond material_upload's own always-
/// global shape — `upload_role`/`subject_id`/`journey_id` are now
/// caller-supplied, so `write_through_prompt_upload` below can reuse
/// this same write+chroma_id-backfill logic for its own journey-scoped
/// documents instead of duplicating it.
///
/// deferred.md #37 — real bug found live once a real reachable
/// ai_service finally let this backfill UPDATE actually run for the
/// first time (every prior attempt failed at add_chunks() itself and
/// returned before reaching it): running it against a bare
/// `&state.pool` connection, outside any RLS transaction, hit
/// `chunks_mixed_scope`/`sources_mixed_scope`'s own
/// `current_setting('app.current_user_id', true)::uuid` cast with an
/// empty string left over from an EARLIER request's `SET LOCAL` on that
/// same pooled connection (not NULL — pooled connections are reused,
/// unlike a fresh session) — `''::uuid` raises, not just "no rows
/// match." Fixed by wrapping it in the same `begin_rls_transaction`
/// pattern every other RLS-protected query in this codebase already
/// uses, now that `user_id` is a real parameter here too.
#[allow(clippy::too_many_arguments)]
async fn write_chunks_to_knowledge_global(
    state: &AppState,
    user_id: Uuid,
    source_id: Uuid,
    chunks: &[(Uuid, Vec<f32>)],
    upload_role: &str,
    subject_id: Option<Uuid>,
    journey_id: Option<Uuid>,
) {
    let records: Vec<ChunkRecord> = chunks
        .iter()
        .map(|(chunk_id, embedding)| ChunkRecord {
            chunk_id: chunk_id.to_string(),
            embedding: embedding.clone(),
            source_id: source_id.to_string(),
            upload_role: upload_role.to_string(),
            trust_score: USER_UPLOAD_TRUST_SCORE,
            subject_id: subject_id.map(|id| id.to_string()),
            concept_id: None,
            journey_id: journey_id.map(|id| id.to_string()),
            difficulty: None,
            chunk_type: None,
        })
        .collect();

    if let Err(err) = ai_client::add_chunks(&state.http_client, &state.ai_service_url, records).await {
        tracing::warn!(?err, %source_id, upload_role, "failed to write chunks to knowledge_global, continuing");
        return;
    }

    let chunk_ids: Vec<Uuid> = chunks.iter().map(|(id, _)| *id).collect();
    // chroma_id is set to the chunk's own id (string form) — its only
    // purpose is "has this been written to Chroma," not a distinct
    // external identifier (the ChromaDB document id IS chunk_id).
    if let Err(err) = backfill_chroma_ids(state, user_id, &chunk_ids).await {
        tracing::warn!(?err, %source_id, "wrote to knowledge_global but failed to backfill chunks.chroma_id");
    }
}

async fn backfill_chroma_ids(state: &AppState, user_id: Uuid, chunk_ids: &[Uuid]) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    sqlx::query("UPDATE chunks SET chroma_id = chunk_id::text WHERE chunk_id = ANY($1)")
        .bind(chunk_ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Writes one `prompt_upload` staged upload through to Postgres — only
/// callable once a real journey_id exists (memoryless::handlers::convert,
/// deferred.md #17, and now also uploads::handlers::upload's own
/// "commit immediately" journey-mode branch, deferred.md #37), since
/// `sources`' own CHECK constraint requires `upload_scope = 'journey'
/// AND journey_id IS NOT NULL` for this role. Mirrors
/// write_through_material_upload's shape, scoped to journey_id via its
/// own dedup index, `sources_content_hash_journey_unique` (ON
/// `(journey_id, content_hash)` — a syntactically exact match to that
/// index's own WHERE clause is required for Postgres to infer it as the
/// ON CONFLICT target, same requirement already confirmed live for the
/// material_upload sibling above).
///
/// deferred.md #37: now also pushes to ChromaDB (journey-scoped
/// metadata), closing a real pre-existing gap — this function
/// previously only ever wrote Postgres, never reaching the vector store
/// despite PRD.md's own "journey mode: store immediately (ChromaDB +
/// PostgreSQL)" line. Postgres write stays hard-propagated (`?`,
/// required by the `convert()` caller's own "commit-then-delete"
/// guarantee); the ChromaDB write stays fail-soft, same posture as its
/// material_upload sibling.
pub async fn write_through_prompt_upload(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    subject_id: Uuid,
    upload: &StagedUpload,
) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    let source_id: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO sources \
         (filename, source_type, upload_role, upload_scope, journey_id, trust_score, content_hash, retrieval_date) \
         VALUES ($1, 'user_upload', 'prompt_upload', 'journey', $2, $3, $4, $5) \
         ON CONFLICT (journey_id, content_hash) WHERE upload_role = 'prompt_upload' AND content_hash IS NOT NULL \
         DO NOTHING \
         RETURNING source_id",
    )
    .bind(&upload.filename)
    .bind(journey_id)
    .bind(USER_UPLOAD_TRUST_SCORE)
    .bind(&upload.content_hash)
    .bind(upload.created_at)
    .fetch_optional(&mut *tx)
    .await?;

    let mut inserted_chunks: Vec<(Uuid, Vec<f32>)> = Vec::new();
    let source_id_for_metadata = source_id.map(|(id,)| id);
    if let Some(source_id) = source_id_for_metadata {
        for chunk in &upload.chunks {
            let chunk_id: Uuid = sqlx::query_scalar(
                "INSERT INTO chunks (source_id, text, token_count) VALUES ($1, $2, $3) RETURNING chunk_id",
            )
            .bind(source_id)
            .bind(&chunk.text)
            .bind(chunk.token_count)
            .fetch_one(&mut *tx)
            .await?;
            inserted_chunks.push((chunk_id, chunk.embedding.clone()));
        }
    }

    tx.commit().await?;

    if let (Some(source_id), false) = (source_id_for_metadata, inserted_chunks.is_empty()) {
        write_chunks_to_knowledge_global(
            state,
            user_id,
            source_id,
            &inserted_chunks,
            "prompt_upload",
            Some(subject_id),
            Some(journey_id),
        )
        .await;
    }

    Ok(())
}
