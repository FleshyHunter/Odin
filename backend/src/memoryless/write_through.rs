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
use crate::auth::middleware::begin_rls_transaction;
use crate::memoryless::errors::MemorylessError;

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
pub async fn write_through_material_upload(
    pool: &PgPool,
    user_id: Uuid,
    upload: &StagedUpload,
) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;

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
         (filename, source_type, upload_role, upload_scope, trust_score, content_hash, retrieval_date) \
         VALUES ($1, 'user_upload', 'material_upload', 'global', $2, $3, $4) \
         ON CONFLICT (content_hash) WHERE upload_role = 'material_upload' AND content_hash IS NOT NULL \
         DO NOTHING \
         RETURNING source_id",
    )
    .bind(&upload.filename)
    .bind(USER_UPLOAD_TRUST_SCORE)
    .bind(&upload.content_hash)
    .bind(upload.created_at)
    .fetch_optional(&mut *tx)
    .await?;

    // A genuine conflict (no row returned) means another insert already
    // owns this content_hash — nothing left to chunk against here.
    if let Some((source_id,)) = source_id {
        for chunk in &upload.chunks {
            sqlx::query("INSERT INTO chunks (source_id, text, token_count) VALUES ($1, $2, $3)")
                .bind(source_id)
                .bind(&chunk.text)
                .bind(chunk.token_count)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

/// Writes one `prompt_upload` staged upload through to Postgres — only
/// callable once a real journey_id exists (memoryless::handlers::convert,
/// deferred.md #17), since `sources`' own CHECK constraint requires
/// `upload_scope = 'journey' AND journey_id IS NOT NULL` for this role.
/// Mirrors write_through_material_upload's shape exactly, just scoped to
/// journey_id via its own dedup index, `sources_content_hash_journey_unique`
/// (ON `(journey_id, content_hash)` — a syntactically exact match to that
/// index's own WHERE clause is required for Postgres to infer it as the
/// ON CONFLICT target, same requirement already confirmed live for the
/// material_upload sibling above).
pub async fn write_through_prompt_upload(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
    upload: &StagedUpload,
) -> Result<(), MemorylessError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;

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

    if let Some((source_id,)) = source_id {
        for chunk in &upload.chunks {
            sqlx::query("INSERT INTO chunks (source_id, text, token_count) VALUES ($1, $2, $3)")
                .bind(source_id)
                .bind(&chunk.text)
                .bind(chunk.token_count)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
