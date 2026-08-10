// deferred.md #18: the read half of real ChromaDB knowledge_global
// retrieval — shared between memoryless::turn and journeys::turn, both
// of which need the identical "query the global KB, then resolve the
// matched chunk_ids' actual TEXT from Postgres" step. `chunk_id` is
// designed to match `chunks.chunk_id` exactly (ARCHITECTURE.md's
// ChromaDB Collection lock) — ai_service's knowledge_global documents
// carry embeddings/metadata only, never the chunk's own text, so the
// real content always comes from Postgres, not Chroma.
//
// The WRITE half (nothing populates knowledge_global with real chunks
// yet) is explicitly out of scope here — see deferred.md #18's own
// entry for the split.

use uuid::Uuid;

use crate::ai_client::{self, AiClientError};
use crate::auth::middleware::begin_rls_transaction;
use crate::state::AppState;

// Fixed implementation constant, not a Rule 12 env var — same reasoning
// already used for turn.rs's own HISTORY_WINDOW_MESSAGES/
// CHUNK_INACTIVITY_TIMEOUT: a prompt-shape decision, not an operational
// knob any deployment needs to tune.
const RETRIEVAL_TOP_K: usize = 5;

/// Queries the permanent, shared `knowledge_global` ChromaDB collection
/// and resolves any matches' actual text from Postgres. `subject_id`
/// scopes the search (journey mode); `None` searches unscoped
/// (memoryless mode — ARCHITECTURE.md's "cross-subject tangent
/// retrieval"). Returns `Ok(None)` for "nothing relevant found," same
/// shape as `memoryless::turn::retrieve_staged_context`.
///
/// Two different failure granularities, both fail-open but handled at
/// the right level: an ai_service/embedding failure is the CALLER's to
/// absorb (bubbled here via `?`, matching #19's own established
/// pattern — the caller already knows how to log it in context); a
/// Postgres failure resolving matched chunks' text is absorbed HERE
/// instead, since by that point ai_service already did the real
/// retrieval work and a DB hiccup fetching text shouldn't discard it.
pub(crate) async fn query_global_context(
    state: &AppState,
    user_id: Uuid,
    query_embedding: &[f32],
    subject_id: Option<Uuid>,
) -> Result<Option<String>, AiClientError> {
    let matches = ai_client::query_knowledge_global(
        &state.http_client,
        &state.ai_service_url,
        query_embedding.to_vec(),
        subject_id,
        RETRIEVAL_TOP_K,
    )
    .await?;

    if matches.is_empty() {
        return Ok(None);
    }

    let chunk_ids: Vec<Uuid> = matches.iter().filter_map(|m| Uuid::parse_str(&m.chunk_id).ok()).collect();
    if chunk_ids.is_empty() {
        return Ok(None);
    }

    let rows = async {
        let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT chunk_id, text FROM chunks WHERE chunk_id = ANY($1)")
                .bind(&chunk_ids)
                .fetch_all(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(rows)
    }
    .await;

    let rows = match rows {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(?err, "failed to resolve retrieved chunk text from Postgres, continuing without it");
            return Ok(None);
        }
    };

    if rows.is_empty() {
        return Ok(None);
    }

    Ok(Some(rows.into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n\n")))
}
