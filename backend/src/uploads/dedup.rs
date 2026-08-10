// Duplicate Upload Prevention (PRD.md) — content_hash computed
// IMMEDIATELY on file arrival, before ai_service ever runs OCR/
// extraction/chunk/embed, checked against three scopes:
//   1. Globally-visible material_upload rows (any uploader)
//   2. The CALLER's OWN prompt_upload rows (never another user's)
//   3. This session's own Redis-staged uploads
// Scopes 1+2 are enforced by reusing the EXISTING sources_mixed_scope
// RLS policy (migrations/0002_row_level_security.sql) via an
// RLS-scoped SELECT, rather than re-deriving the identical scoping
// logic by hand in a WHERE clause — the policy already encodes exactly
// these two rules.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::middleware::begin_rls_transaction;
use crate::memoryless::errors::MemorylessError;
use crate::memoryless::staging::{StagedThread, StagedUpload};

pub fn compute_content_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Scope 3 — this session's own staged uploads (Redis, already loaded
/// in memory). Catches the memoryless refresh/re-click case.
pub fn find_staged<'a>(thread: &'a StagedThread, content_hash: &str) -> Option<&'a StagedUpload> {
    thread.staged_uploads.iter().find(|u| u.content_hash == content_hash)
}

/// Scopes 1+2 — committed sources, RLS-scoped to (material_upload rows
/// from anyone) OR (prompt_upload rows the caller owns). In practice,
/// the prompt_upload half of this can't find anything yet for ANY real
/// user — no journey ever gets created by any code path (markdown/
/// deferred.md #4/#27), so no prompt_upload row can even exist to
/// match against. material_upload's half works fully today.
pub async fn find_committed(pool: &PgPool, user_id: Uuid, content_hash: &str) -> Result<Option<Uuid>, MemorylessError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT source_id FROM sources WHERE content_hash = $1 LIMIT 1")
        .bind(content_hash)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(row.map(|(id,)| id))
}

/// deferred.md #37 — journey-mode's own dedup scope for `prompt_upload`,
/// deliberately NOT the same as `find_committed` above: the same file
/// uploaded to two different journeys is two legitimate, separate rows
/// (the real `sources_content_hash_journey_unique` index is scoped to
/// `(journey_id, content_hash)`, not globally unique) — `find_committed`
/// would wrongly report a duplicate across journeys here.
pub async fn find_committed_in_journey(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
    content_hash: &str,
) -> Result<Option<Uuid>, MemorylessError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT source_id FROM sources \
         WHERE journey_id = $1 AND content_hash = $2 AND upload_role = 'prompt_upload' LIMIT 1",
    )
    .bind(journey_id)
    .bind(content_hash)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.map(|(id,)| id))
}
