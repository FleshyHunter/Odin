// Real journey-mode chat turn loop (deferred.md #2a) — Flow 4's own
// "teach from first concept" tail, split out of #2 since nothing else in
// #2 (the off-topic/DAG-gap classifier, #2b+) has a real message loop to
// run against without this existing first. Mirrors memoryless/turn.rs's
// shape closely (SSE streaming via a spawned task, capped history
// window, one audit_logs row per turn, Rule 4) with one structural
// difference: a journey thread has no staging step at all — Rule 11
// already treats it as real from message one, so this writes directly to
// Postgres as the ONLY path, no Redis, no write-through catch-up.
//
// Wired against the permanent knowledge base as of deferred.md #18 (see
// query_knowledge_context) — subject-scoped, unlike memoryless mode's
// unscoped search. Still bare against mastery/exercises (a separate,
// already-real system) — same incremental-build philosophy memoryless
// itself was built under (Block 11 first, retrieval/history added
// later). TANGENT/OUT_OF_SCOPE turns are detected and recorded
// faithfully (this is the first real caller of analyze_input's non-None
// current_concept_id case) but given no special Flow 3 handling yet —
// that's deferred.md #2b+'s own job once this exists for it to hook into.

use std::time::Duration;

use chrono::Utc;
use futures_util::{Stream, StreamExt};
use sqlx::PgPool;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai_client::{self, AiClientError, ConceptMeta, HistoryMessage};
use crate::auth::middleware::begin_rls_transaction;
use crate::auth::rate_limit;
use crate::exercises;
use crate::knowledge;
use crate::state::AppState;

use super::errors::JourneyError;

// Matches ai_service/app/generation/service.py's MODEL_NAME literal —
// same reasoning as memoryless/turn.rs's own identical constant.
const MODEL_USED: &str = "qwen3.5:9b";
const CHUNK_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);
const HISTORY_WINDOW_MESSAGES: usize = 10;

pub enum TurnEvent {
    Delta(String),
    Error(String),
}

pub(crate) struct EntryConceptInfo {
    pub(crate) concept_id: Uuid,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) learning_objective: Option<String>,
}

pub struct ThreadHydration {
    pub thread_id: Uuid,
    pub current_concept_title: String,
    pub messages: Vec<(String, String, chrono::DateTime<Utc>)>, // (role, content, timestamp)
}

/// RLS-scoped — confirms the journey belongs to this user AND returns
/// its subject_id in one step; every other lookup below needs both.
pub(crate) async fn verify_journey_and_subject(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
) -> Result<Uuid, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT subject_id FROM journeys WHERE journey_id = $1")
        .bind(journey_id)
        .fetch_optional(&mut *tx)
        .await?;
    tx.commit().await?;
    row.map(|(id,)| id).ok_or(JourneyError::NotFound)
}

async fn find_thread(pool: &PgPool, user_id: Uuid, journey_id: Uuid) -> Result<Option<Uuid>, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT thread_id FROM study_threads WHERE journey_id = $1 AND mode = 'journey'")
            .bind(journey_id)
            .fetch_optional(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(row.map(|(id,)| id))
}

/// journey_concepts (RLS) joined against subject_concepts/canonical_concepts
/// (globally shared, not RLS) in one query, run inside the RLS
/// transaction — safe: RLS only restricts journey_concepts' own rows,
/// the joined tables are unaffected either way.
pub(crate) async fn fetch_entry_concept(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
    subject_id: Uuid,
    dag_version: i32,
) -> Result<EntryConceptInfo, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let row: Option<(Uuid, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT cc.concept_id, cc.title, cc.description, sc.learning_objective \
         FROM journey_concepts jc \
         JOIN canonical_concepts cc ON cc.concept_id = jc.concept_id \
         JOIN subject_concepts sc ON sc.concept_id = jc.concept_id AND sc.subject_id = $2 AND sc.dag_version = $3 \
         WHERE jc.journey_id = $1 AND jc.status = 'available'",
    )
    .bind(journey_id)
    .bind(subject_id)
    .bind(dag_version)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let (concept_id, title, description, learning_objective) = row.ok_or(JourneyError::Internal)?;
    Ok(EntryConceptInfo { concept_id, title, description, learning_objective })
}

/// deferred.md #26 — the "1-3 immediate child concepts" exercise-template
/// generation wants for context (ai_service/app/acquisition/service.py's
/// batch_children_block). RLS-scoped: journey_prerequisites is per-journey,
/// same table shape as subject_prerequisites just without dag_version.
async fn fetch_batch_children(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
    concept_id: Uuid,
) -> Result<Vec<Uuid>, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT concept_id FROM journey_prerequisites WHERE journey_id = $1 AND prereq_concept_id = $2",
    )
    .bind(journey_id)
    .bind(concept_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// deferred.md #26 — "the moment a concept becomes the active teaching
/// target... the template call fires concurrently" (PRD.md, Exercise
/// Template Generation). Fire-and-forget: exercise-template prefetch is
/// an optimization, not core to the turn succeeding, same posture as
/// every other write-through call in this codebase. `top_chunks` is
/// deliberately empty, not a shortcut — write_through.rs already
/// documents that no chunk anywhere in the live system has a concept_id
/// assigned yet (explicit scope cut; "misc chunk, still retrievable,
/// just unassigned" is a valid PRD state).
fn spawn_exercise_prefetch(state: AppState, user_id: Uuid, journey_id: Uuid, entry: &EntryConceptInfo) {
    let concept_id = entry.concept_id;
    let concept_meta = ConceptMeta {
        title: entry.title.clone(),
        description: entry.description.clone().unwrap_or_default(),
    };
    tokio::spawn(async move {
        let batch_children = match fetch_batch_children(&state.pool, user_id, journey_id, concept_id).await {
            Ok(children) => children,
            Err(err) => {
                tracing::warn!(?err, %concept_id, "exercise-template prefetch: failed to fetch batch_children, proceeding without them");
                Vec::new()
            }
        };
        if let Err(err) =
            exercises::service::get_or_generate(&state, concept_id, concept_meta, Vec::new(), batch_children).await
        {
            tracing::warn!(?err, %concept_id, "exercise-template prefetch-on-arrival failed");
        }
    });
}

async fn fetch_known_terms(pool: &PgPool, subject_id: Uuid, dag_version: i32) -> Result<Vec<String>, JourneyError> {
    let titles: Vec<(String,)> = sqlx::query_as(
        "SELECT cc.title FROM subject_concepts sc \
         JOIN canonical_concepts cc ON cc.concept_id = sc.concept_id \
         WHERE sc.subject_id = $1 AND sc.dag_version = $2",
    )
    .bind(subject_id)
    .bind(dag_version)
    .fetch_all(pool)
    .await?;
    Ok(titles.into_iter().map(|(t,)| t).collect())
}

/// deferred.md #2b — display name for ai_service's Stage 2 classify_gap()
/// prompt. Not RLS-scoped — canonical_concepts is globally shared, same
/// as fetch_known_terms/fetch_entry_concept's own joins against it.
async fn fetch_concept_title(pool: &PgPool, concept_id: Uuid) -> Result<String, JourneyError> {
    let (title,): (String,) = sqlx::query_as("SELECT title FROM canonical_concepts WHERE concept_id = $1")
        .bind(concept_id)
        .fetch_one(pool)
        .await?;
    Ok(title)
}

/// RLS-scoped. Real Postgres history (not a Redis-staged Vec, unlike
/// memoryless) — capped the same way build_history() windows it, just
/// windowed by the SQL query itself (last N by timestamp, re-ordered
/// back to chronological after).
async fn load_recent_history(
    pool: &PgPool,
    user_id: Uuid,
    thread_id: Uuid,
) -> Result<Vec<HistoryMessage>, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM messages WHERE thread_id = $1 \
         ORDER BY timestamp DESC LIMIT $2",
    )
    .bind(thread_id)
    .bind(HISTORY_WINDOW_MESSAGES as i64)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(rows
        .into_iter()
        .rev() // DESC-then-reverse == the last N in chronological order
        .map(|(role, content)| HistoryMessage {
            role: if role == "tutor" { "assistant".to_string() } else { role },
            content,
        })
        .collect())
}

// deferred.md #18: the permanent, shared knowledge base — subject-
// scoped here (unlike memoryless mode's unscoped search), matching
// ARCHITECTURE.md's "subject-scoped retrieval (filter subject_id)"
// case. Own embed() call, same "not worth sharing across an unrelated
// module boundary" reasoning as memoryless/turn.rs's own query_
// knowledge_context. Fails open on any embedding-service error — a
// retrieval ENHANCEMENT, not core functionality.
//
// journey_id (deferred.md #18's own privacy gap, fixed 2026-08-12):
// passed through so ai_service can keep prompt_upload results scoped
// to THIS journey — this function's own caller already has it via
// verify_journey_and_subject, a real ownership-verified value, not
// trusted from anywhere else.
async fn query_knowledge_context(
    state: &AppState,
    user_id: Uuid,
    subject_id: Uuid,
    journey_id: Uuid,
    query: &str,
) -> Result<Option<String>, AiClientError> {
    let query_embedding = ai_client::embed(&state.http_client, &state.ai_service_url, vec![query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AiClientError::UnexpectedResponse("embed() returned no vectors for the query".to_string()))?;

    knowledge::query_global_context(state, user_id, &query_embedding, Some(subject_id), Some(journey_id)).await
}

// Found live (2026-08-12), same root cause as memoryless mode's own
// all_staged_context fix: journey-mode had no mechanism at all reading
// a journey's own uploaded content back into generation — #37 built the
// WRITE side (Postgres + ChromaDB, "commit immediately") but nothing
// ever read it back. Deliberately a direct Postgres query, not a
// ChromaDB search — always-include, no ranking, no threshold, mirroring
// memoryless's own unconditional inclusion. Picks up BOTH upload roles,
// via TWO different columns (migration 0006, per discussion with V3):
// `prompt_upload` already has `journey_id` set as its real privacy
// scope; `material_upload` carries `origin_journey_id`, a separate
// purely-informational tag, since `journey_id` itself is CHECK-
// constrained to NULL for that role and overloading it would make one
// column mean two different things depending on upload_role.
async fn fetch_journey_upload_context(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
) -> Result<Option<String>, JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT c.text FROM chunks c JOIN sources s ON s.source_id = c.source_id \
         WHERE s.journey_id = $1 OR s.origin_journey_id = $1",
    )
    .bind(journey_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(rows.into_iter().map(|(text,)| text).collect::<Vec<_>>().join("\n\n")))
}

async fn stream_to_completion(
    chunks: impl Stream<Item = Result<String, AiClientError>> + Send + 'static,
    tx: &mpsc::Sender<TurnEvent>,
) -> (String, Option<String>) {
    let mut accumulated = String::new();
    let mut cutoff_reason: Option<String> = None;
    tokio::pin!(chunks);

    loop {
        // Checked FIRST (biased) and independently of chunks.next()
        // below — same fix as memoryless/turn.rs's identical loop
        // (deferred.md, found live 2026-08-10): the old code only ever
        // noticed a cancelled turn reactively, via tx.send() failing,
        // which only runs once chunks.next() actually yields something.
        // For a "thinking" turn where ai_service/Ollama takes a while to
        // produce even its first token, this loop would sit fully
        // blocked on chunks.next() the whole time, unable to notice a
        // cancellation that had already happened — confirmed live, over
        // 13s of undetected delay in memoryless mode's own copy of this
        // exact loop before this fix. tx.closed() resolves the instant
        // the receiver (owned by the SSE stream in handlers.rs) is
        // dropped, independent of whether anything is currently flowing
        // through the channel.
        tokio::select! {
            biased;
            _ = tx.closed() => {
                cutoff_reason = Some("cancelled by user".to_string());
                break;
            }
            result = tokio::time::timeout(CHUNK_INACTIVITY_TIMEOUT, chunks.next()) => {
                match result {
                    Ok(Some(Ok(delta))) => {
                        accumulated.push_str(&delta);
                        // tx.closed() above already covers the
                        // cancellation case — this remains as a harmless
                        // fallback for the rare timing where both
                        // branches raced ready at once and this one won.
                        if tx.send(TurnEvent::Delta(delta)).await.is_err() {
                            cutoff_reason = Some("cancelled by user".to_string());
                            break;
                        }
                    }
                    Ok(Some(Err(err))) => {
                        let reason = format!("ai service error mid-stream: {err}");
                        let _ = tx.send(TurnEvent::Error(reason.clone())).await;
                        cutoff_reason = Some(reason);
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        let reason = format!("generation stalled — no output for {}s", CHUNK_INACTIVITY_TIMEOUT.as_secs());
                        let _ = tx.send(TurnEvent::Error(reason.clone())).await;
                        cutoff_reason = Some(reason);
                        break;
                    }
                }
            }
        }
    }
    (accumulated, cutoff_reason)
}

/// POST /journeys/{journey_id}/start — one-time call. Errors if a thread
/// already exists (the frontend should GET first). No analyze_input call
/// — there's no student text to clean, this is genuinely tutor-initiated.
pub async fn start_journey_thread(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    think: bool,
) -> Result<(Uuid, String, mpsc::Receiver<TurnEvent>), JourneyError> {
    if find_thread(&state.pool, user_id, journey_id).await?.is_some() {
        return Err(JourneyError::Validation(
            "this journey's teaching thread has already started".to_string(),
        ));
    }

    let subject_id = verify_journey_and_subject(&state.pool, user_id, journey_id).await?;
    let dag_version: i32 = sqlx::query_scalar("SELECT dag_version FROM subjects WHERE subject_id = $1")
        .bind(subject_id)
        .fetch_one(&state.pool)
        .await?;
    let entry = fetch_entry_concept(&state.pool, user_id, journey_id, subject_id, dag_version).await?;

    // deferred.md #26: fired here, not after the stream completes below —
    // this is the exact moment the entry concept becomes the active
    // teaching target, and PRD.md wants the template call concurrent with
    // the tutor's own opening-turn generation, not sequential after it.
    spawn_exercise_prefetch(state.clone(), user_id, journey_id, &entry);

    let prompt = format!(
        "You are a tutor starting a brand-new session with a student. Introduce and begin teaching \
         the concept \"{}\".{}{} Write a warm, engaging opening that introduces the concept and starts \
         teaching it — the student hasn't said anything yet, so don't wait for a response or ask what \
         they want to learn; just begin.",
        entry.title,
        entry.description.as_ref().map(|d| format!(" Description: {d}.")).unwrap_or_default(),
        entry.learning_objective.as_ref().map(|o| format!(" Learning objective: {o}.")).unwrap_or_default(),
    );

    let chunks =
        ai_client::generate_stream(&state.streaming_http_client, &state.ai_service_url, prompt, think, Vec::new())
            .await?;

    let thread_id = Uuid::new_v4();
    let (tx, rx) = mpsc::channel::<TurnEvent>(16);
    let state = state.clone();
    let concept_id = entry.concept_id;
    let concept_title = entry.title.clone();

    tokio::spawn(async move {
        let (accumulated, cutoff_reason) = stream_to_completion(chunks, &tx).await;
        let now = Utc::now();

        if let Err(err) = persist_opening_turn(
            &state.pool,
            user_id,
            thread_id,
            journey_id,
            concept_id,
            now,
            &accumulated,
            cutoff_reason.as_deref(),
        )
        .await
        {
            tracing::error!(?err, %journey_id, %thread_id, "failed to persist journey opening turn");
        }
    });

    Ok((thread_id, concept_title, rx))
}

/// Reused by tracks::handlers::create_track — a new Track needs a real,
/// stable id (the thread_id) the moment it's created, not later when
/// chat first opens, so this is the same generation start_journey_thread
/// does, just awaited to full completion here instead of streamed back
/// to a live SSE client, and inserted atomically WITH its opening
/// message in one go — a thread created this way never exists with zero
/// messages, so useJourneyChat's existing hydration path (thread found →
/// hydrate) needs no changes at all; its lazy "no thread yet" branch
/// simply never fires for Tracks created this way. Also sets `title`,
/// which start_journey_thread's own persist_opening_turn never has a
/// reason to (that path's thread always falls back to the journey's
/// subject title, per SCHEMA.md's own comment on study_threads.title).
pub(crate) async fn create_journey_thread_sync(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    title: &str,
) -> Result<(Uuid, String), JourneyError> {
    let subject_id = verify_journey_and_subject(&state.pool, user_id, journey_id).await?;
    let dag_version: i32 = sqlx::query_scalar("SELECT dag_version FROM subjects WHERE subject_id = $1")
        .bind(subject_id)
        .fetch_one(&state.pool)
        .await?;
    let entry = fetch_entry_concept(&state.pool, user_id, journey_id, subject_id, dag_version).await?;

    // deferred.md #26: same "fire concurrently with the opening turn's
    // own generation" placement as start_journey_thread's own call.
    spawn_exercise_prefetch(state.clone(), user_id, journey_id, &entry);

    let prompt = format!(
        "You are a tutor starting a brand-new session with a student. Introduce and begin teaching \
         the concept \"{}\".{}{} Write a warm, engaging opening that introduces the concept and starts \
         teaching it — the student hasn't said anything yet, so don't wait for a response or ask what \
         they want to learn; just begin.",
        entry.title,
        entry.description.as_ref().map(|d| format!(" Description: {d}.")).unwrap_or_default(),
        entry.learning_objective.as_ref().map(|o| format!(" Learning objective: {o}.")).unwrap_or_default(),
    );

    let chunks = ai_client::generate_stream(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        true, // same default as start_journey_thread's HTTP handler (body.think.unwrap_or(true))
        Vec::new(),
    )
    .await?;
    tokio::pin!(chunks);

    let mut accumulated = String::new();
    while let Some(chunk) = chunks.next().await {
        accumulated.push_str(&chunk?);
    }

    let thread_id = Uuid::new_v4();
    let now = Utc::now();
    let concept_id = entry.concept_id;
    let concept_title = entry.title;

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    sqlx::query(
        "INSERT INTO study_threads \
         (thread_id, user_id, journey_id, title, mode, current_concept_id, created_at, last_active_at) \
         VALUES ($1, $2, $3, $4, 'journey', $5, $6, $6)",
    )
    .bind(thread_id)
    .bind(user_id)
    .bind(journey_id)
    .bind(title)
    .bind(concept_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // "Teach from first concept" (Flow 4) — same as persist_opening_turn.
    sqlx::query("UPDATE journey_concepts SET status = 'in_progress' WHERE journey_id = $1 AND concept_id = $2")
        .bind(journey_id)
        .bind(concept_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("INSERT INTO messages (thread_id, role, content, mode, timestamp) VALUES ($1, 'tutor', $2, 'journey', $3)")
        .bind(thread_id)
        .bind(&accumulated)
        .bind(now)
        .execute(&mut *tx)
        .await?;

    // Rule 4: mandatory every turn, all modes.
    sqlx::query(
        "INSERT INTO audit_logs (thread_id, mode, response_text, model_used, error, timestamp) \
         VALUES ($1, 'journey', $2, $3, NULL, $4)",
    )
    .bind(thread_id)
    .bind(&accumulated)
    .bind(MODEL_USED)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok((thread_id, concept_title))
}

#[allow(clippy::too_many_arguments)]
async fn persist_opening_turn(
    pool: &PgPool,
    user_id: Uuid,
    thread_id: Uuid,
    journey_id: Uuid,
    concept_id: Uuid,
    now: chrono::DateTime<Utc>,
    response_text: &str,
    error: Option<&str>,
) -> Result<(), JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;

    sqlx::query(
        "INSERT INTO study_threads (thread_id, user_id, journey_id, mode, current_concept_id, created_at, last_active_at) \
         VALUES ($1, $2, $3, 'journey', $4, $5, $5)",
    )
    .bind(thread_id)
    .bind(user_id)
    .bind(journey_id)
    .bind(concept_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // "Teach from first concept" (Flow 4) — the entry concept genuinely
    // starts being taught now, not just made available.
    sqlx::query("UPDATE journey_concepts SET status = 'in_progress' WHERE journey_id = $1 AND concept_id = $2")
        .bind(journey_id)
        .bind(concept_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query("INSERT INTO messages (thread_id, role, content, mode, timestamp) VALUES ($1, 'tutor', $2, 'journey', $3)")
        .bind(thread_id)
        .bind(response_text)
        .bind(now)
        .execute(&mut *tx)
        .await?;

    // Rule 4: mandatory every turn, all modes — user_input/cleaned_query
    // NULL is correct here, not an omission: nothing was said yet.
    sqlx::query(
        "INSERT INTO audit_logs (thread_id, mode, response_text, model_used, error, timestamp) \
         VALUES ($1, 'journey', $2, $3, $4, $5)",
    )
    .bind(thread_id)
    .bind(response_text)
    .bind(MODEL_USED)
    .bind(error)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// deferred.md #2c — persists ONE new concept into a specific journey's
/// DAG. Two-phase, mirroring journeys::service::finalize()'s exact same
/// shape for a whole new subject, just for one concept: canonical_concepts
/// (global, plain pool, same "globally shared, not RLS" reasoning as
/// persist_new_subject) first, then journey_concepts/journey_prerequisites
/// (RLS, this student's own journey only) in one transaction. Deliberately
/// scoped to journey_prerequisites, NOT subject_prerequisites/
/// subject_concepts (deferred.md #2c's own decided dag_version semantics:
/// personal to this student, not canonical) — no dag_version bump, no
/// other journey on this subject is ever affected. New concept starts
/// 'available' immediately (the student is actively asking about it right
/// now), same status the entry concept itself starts with.
async fn fold_concept_into_journey(
    pool: &PgPool,
    user_id: Uuid,
    journey_id: Uuid,
    subject_id: Uuid,
    dag_version: i32,
    current_concept_id: Uuid,
    folded: &ai_client::FoldedConcept,
) -> Result<Uuid, JourneyError> {
    let concept_id: Uuid = sqlx::query_scalar(
        "INSERT INTO canonical_concepts (title, description) VALUES ($1, $2) RETURNING concept_id",
    )
    .bind(&folded.title)
    .bind(&folded.description)
    .fetch_one(pool)
    .await?;

    // Resolve prerequisite titles -> concept_ids against THIS subject's
    // existing concepts — same title-based convention as ConceptNode's
    // own prerequisites, same defensive "skip a dangling reference"
    // fallback as persist_new_subject's own loop (a folded concept
    // having fewer prerequisites than intended isn't catastrophic, unlike
    // a whole corrupted DAG). current_concept_id is always included even
    // if the LLM didn't name it by title — the gap was raised while the
    // student was on it.
    let mut prereq_ids: Vec<Uuid> = Vec::new();
    for title in &folded.prerequisites {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT cc.concept_id FROM subject_concepts sc \
             JOIN canonical_concepts cc ON cc.concept_id = sc.concept_id \
             WHERE sc.subject_id = $1 AND sc.dag_version = $2 AND cc.title = $3",
        )
        .bind(subject_id)
        .bind(dag_version)
        .bind(title)
        .fetch_optional(pool)
        .await?;
        match existing {
            Some((id,)) => prereq_ids.push(id),
            None => tracing::warn!(%title, "fold_concept_into_journey: dangling prerequisite title, skipping"),
        }
    }
    if !prereq_ids.contains(&current_concept_id) {
        prereq_ids.push(current_concept_id);
    }

    let mut tx = begin_rls_transaction(pool, user_id).await?;

    let order_index: i32 =
        sqlx::query_scalar("SELECT COALESCE(MAX(order_index), 0) + 1 FROM journey_concepts WHERE journey_id = $1")
            .bind(journey_id)
            .fetch_one(&mut *tx)
            .await?;

    sqlx::query(
        "INSERT INTO journey_concepts (journey_id, concept_id, status, order_index) VALUES ($1, $2, 'available', $3)",
    )
    .bind(journey_id)
    .bind(concept_id)
    .bind(order_index)
    .execute(&mut *tx)
    .await?;

    for prereq_id in &prereq_ids {
        sqlx::query("INSERT INTO journey_prerequisites (journey_id, concept_id, prereq_concept_id) VALUES ($1, $2, $3)")
            .bind(journey_id)
            .bind(concept_id)
            .bind(prereq_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(concept_id)
}

/// deferred.md #2c — the fold_gap trigger: calls ai_service's fold
/// capability, then persists the result via fold_concept_into_journey
/// above. Returns the new concept's title on success, for the SAME
/// turn's response to briefly acknowledge (deferred.md #2e: "case
/// (b)-fold probably wants SOME acknowledgment, even if it's small").
/// Callers are expected to fail soft on any Err — a failed fold costs
/// nothing beyond staying a plain TANGENT for this one turn, same as a
/// failed classify_gap() Stage 2 call already does.
#[allow(clippy::too_many_arguments)]
async fn fold_gap_into_journey(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    subject_id: Uuid,
    dag_version: i32,
    current_concept_id: Uuid,
    subject_title: &str,
    concept_titles: &[String],
    current_concept_title: &str,
    reply_text: &str,
) -> Result<String, JourneyError> {
    let folded = ai_client::fold_concept_into_dag(
        &state.http_client,
        &state.ai_service_url,
        subject_title.to_string(),
        concept_titles.to_vec(),
        current_concept_title.to_string(),
        reply_text.to_string(),
    )
    .await?;

    fold_concept_into_journey(
        &state.pool,
        user_id,
        journey_id,
        subject_id,
        dag_version,
        current_concept_id,
        &folded,
    )
    .await?;

    Ok(folded.title)
}

/// deferred.md #2d — clears study_threads.pending_branch_topic once a
/// branch offer is resolved (confirmed or declined), so a later,
/// unrelated message never gets misread as answering a stale offer.
async fn clear_pending_branch_topic(pool: &PgPool, user_id: Uuid, thread_id: Uuid) -> Result<(), JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    sqlx::query("UPDATE study_threads SET pending_branch_topic = NULL WHERE thread_id = $1")
        .bind(thread_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// deferred.md #2d — records the offer the SAME turn a branch_gap fires,
/// so the student's NEXT message can be read as answering it.
async fn set_pending_branch_topic(pool: &PgPool, user_id: Uuid, thread_id: Uuid, topic: &str) -> Result<(), JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;
    sqlx::query("UPDATE study_threads SET pending_branch_topic = $2 WHERE thread_id = $1")
        .bind(thread_id)
        .bind(topic)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// deferred.md #2d — a deliberately loose, rules-based yes/no read on
/// the branch-offer confirmation turn, not a full intent classifier —
/// same "cheap rules first" philosophy as ai_service's own
/// classify_intent, just narrower (one binary decision, no qwen
/// fallback). Errs toward requiring a recognizable affirmative: an
/// unmatched reply is treated as declined and just falls through to a
/// normal turn — never silently creates a journey on an ambiguous
/// read.
///
/// deferred.md #84/V3 audit bug #4: originally a reply that STARTS with
/// an affirmative word but is actually declining ("yeah, no thanks")
/// misread as confirmed — accepted at the time as low-stakes since the
/// created journey was "deletable like any other track." That premise
/// turned out false (no delete path existed anywhere — #84), so this
/// now guards against a negation word landing immediately after the
/// matched affirmative ("yeah, no thanks" -> declined). Deliberately
/// narrow (adjacency only, not a whole-string negation scan) so a
/// negation word appearing LATER in a genuine yes doesn't misfire
/// ("yes, let's do it, no doubt about it" still confirms — "no" isn't
/// adjacent to "yes"). Known, accepted remaining gap: an idiom where
/// "no" immediately follows an affirmative WITHOUT negating it ("yeah,
/// no doubt, let's do it") still misreads as declined — genuinely
/// ambiguous without real intent classification, not worth one for
/// this low-stakes a decision either.
fn is_branch_confirmed(text: &str) -> bool {
    const AFFIRMATIVE: &[&str] = &[
        "yes", "yeah", "yep", "yup", "sure", "ok", "okay", "alright", "please", "go for it", "do it",
        "sounds good", "lets do it", "let's do it", "start it", "i guess", "go ahead", "please do",
    ];
    const NEGATION: &[&str] = &["no", "not", "nope", "nah", "dont", "don't"];

    let normalized: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'')
        .collect();

    AFFIRMATIVE.iter().any(|phrase| {
        if normalized == *phrase {
            return true;
        }
        let Some(rest) = normalized.strip_prefix(&format!("{phrase} ")) else {
            return false;
        };
        !NEGATION.iter().any(|neg| rest == *neg || rest.starts_with(&format!("{neg} ")))
    })
}

/// deferred.md #2d — the confirmation-only turn once a branch has just
/// been created: a short, tutor-voiced acknowledgment. Deliberately
/// skips the normal analyze_input/retrieval pipeline entirely — there's
/// no student academic question here to clean or retrieve context for,
/// this message was purely a yes/no answer to an offer. Reuses
/// persist_turn (not a new persistence path) — the confirmation is
/// still a normal message/audit_logs pair on THIS journey's thread; the
/// newly created journey gets its own thread lazily via its own
/// /start call, same as any other journey.
async fn respond_to_confirmed_branch(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    thread_id: Uuid,
    topic: &str,
    raw_input: String,
    think: bool,
) -> Result<mpsc::Receiver<TurnEvent>, JourneyError> {
    let prompt = format!(
        "(You just started a new, separate learning track for the student on \"{topic}\" — briefly \
         confirm this in one or two warm sentences, and let them know they can switch to it from the \
         sidebar whenever they're ready. Do not start teaching the new topic here — that happens in \
         the new track.)"
    );

    let history = load_recent_history(&state.pool, user_id, thread_id).await?;
    let chunks = ai_client::generate_stream(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        think,
        history,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<TurnEvent>(16);
    let state = state.clone();

    tokio::spawn(async move {
        let (accumulated, cutoff_reason) = stream_to_completion(chunks, &tx).await;
        let now = Utc::now();

        if let Err(err) = persist_turn(
            &state.pool,
            user_id,
            thread_id,
            now,
            &raw_input,
            &raw_input, // cleaned_query: no NLP cleaning needed for a yes/no answer
            &[],
            "branch_confirmed",
            &accumulated,
            cutoff_reason.as_deref(),
        )
        .await
        {
            tracing::error!(?err, %journey_id, %thread_id, "failed to persist branch-confirmation turn");
        }
    });

    Ok(rx)
}

// ============================================================
// Exercise loop: serve -> submit -> grade -> mastery/streak/advancement.
// The piece PRD.md's own Flow 2 spec always described ("offer exercise
// -> grade -> completion check -> advance") but that never had a real
// caller until now — exercise TEMPLATES have generated since Block 12,
// and grading has been real and deterministic since Block 9, but
// nothing ever served an instantiated question, accepted a submission,
// or wrote mastery_bank/advanced journey_concepts. This is that.
// ============================================================

pub struct ServedExercise {
    pub attempt_id: Uuid,
    pub exercise_type: String,
    pub difficulty: String,
    pub rendered_question: String,
    pub rendered_choices: Option<Vec<String>>,
}

/// POST /journeys/{journey_id}/concepts/{concept_id}/exercise — serves a
/// FRESH instantiated question, whether the caller is the live teaching
/// loop offering one or a student self-directing via the Map (deferred.md
/// #2c/#2d's own "Now becomes the single surface either way" framing
/// applies here too — this doesn't care which). Persists a quiz_attempts
/// row immediately with student_answer still NULL — that's what makes
/// the attempt real/auditable the instant it's served, not just on
/// submission, and what submit_exercise_answer below finds and fills in.
pub async fn serve_exercise(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    concept_id: Uuid,
    difficulty: String,
) -> Result<ServedExercise, JourneyError> {
    let thread_id = find_thread(&state.pool, user_id, journey_id)
        .await?
        .ok_or_else(|| JourneyError::Validation("this journey's teaching thread hasn't started yet".to_string()))?;

    let template = exercises::service::fetch_one_canonical(&state.pool, concept_id, &difficulty)
        .await?
        .ok_or_else(|| {
            JourneyError::Validation(format!(
                "no canonical exercise exists yet for this concept at difficulty {difficulty}"
            ))
        })?;
    let exercise_id = exercises::service::fetch_one_canonical_id(&state.pool, concept_id, &difficulty)
        .await?
        .ok_or(JourneyError::Internal)?;

    let instantiated = ai_client::instantiate_exercise(
        &state.http_client,
        &state.ai_service_url,
        template.exercise_type.clone(),
        template.template_body.clone(),
        template.template_params.clone(),
        template.correct_answer.clone(),
    )
    .await?;

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let attempt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO quiz_attempts \
         (user_id, exercise_id, thread_id, journey_id, rendered_question, instantiated_params, \
          expected_answer, difficulty_attempted, timestamp) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING attempt_id",
    )
    .bind(user_id)
    .bind(exercise_id)
    .bind(thread_id)
    .bind(journey_id)
    .bind(&instantiated.rendered_question)
    .bind(&instantiated.instantiated_params)
    .bind(&instantiated.expected_answer)
    .bind(&difficulty)
    .bind(Utc::now())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(ServedExercise {
        attempt_id,
        exercise_type: template.exercise_type,
        difficulty,
        rendered_question: instantiated.rendered_question,
        rendered_choices: instantiated.rendered_choices,
    })
}

#[derive(sqlx::FromRow)]
struct PendingAttempt {
    thread_id: Uuid,
    rendered_question: String,
    expected_answer: Option<String>,
    difficulty_attempted: Option<String>,
    exercise_type: Option<String>,
    concept_id: Option<Uuid>,
    tolerance: Option<f32>,
    grader_config: Option<serde_json::Value>,
}

/// POST /journeys/{journey_id}/exercises/{attempt_id}/submit — grades
/// the answer (Rule 2: always deterministic, ai_client::grade(), never
/// an LLM judgment call), then applies PRD.md's locked Wrong Answer
/// Handling rules and streams the tutor's reaction.
///
/// SCOPE CUT, stated plainly rather than silently: this pass implements
/// the streak/mastery/completion/advancement mechanics that follow
/// directly from a single submission. It does NOT implement
/// foundation_gap flagging on a skipped basic-wrong, or KIV flagging on
/// an advanced-wrong "move on" — both are explicitly conditional on a
/// SEPARATE decision ("if skipped" / "if move on") no frontend
/// affordance exists yet to make. Wrong answers still correctly reset
/// the streak and correctly block completion either way; only those two
/// specific branch-on-a-second-decision flags are deferred.
#[allow(clippy::too_many_arguments)]
pub async fn submit_exercise_answer(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    attempt_id: Uuid,
    student_answer: String,
    think: bool,
) -> Result<mpsc::Receiver<TurnEvent>, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let pending: Option<PendingAttempt> = sqlx::query_as(
        "SELECT qa.thread_id, qa.rendered_question, qa.expected_answer, qa.difficulty_attempted, \
                e.exercise_type, e.concept_id, e.tolerance, e.grader_config \
         FROM quiz_attempts qa \
         JOIN exercises e ON e.exercise_id = qa.exercise_id \
         WHERE qa.attempt_id = $1 AND qa.student_answer IS NULL",
    )
    .bind(attempt_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    let pending = pending
        .ok_or_else(|| JourneyError::Validation("exercise attempt not found or already answered".to_string()))?;

    let exercise_type = pending.exercise_type.clone().ok_or(JourneyError::Internal)?;
    let concept_id = pending.concept_id.ok_or(JourneyError::Internal)?;

    let grade_result = ai_client::grade(
        &state.http_client,
        &state.ai_service_url,
        exercise_type.clone(),
        student_answer.clone(),
        pending.expected_answer.clone(),
        pending.tolerance,
        pending.grader_config.clone(),
    )
    .await?;

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    sqlx::query(
        "UPDATE quiz_attempts SET student_answer = $2, is_correct = $3, score = $4, \
         grader_used = $5, feedback = $6 WHERE attempt_id = $1",
    )
    .bind(attempt_id)
    .bind(&student_answer)
    .bind(grade_result.is_correct)
    .bind(grade_result.score)
    .bind(&exercise_type)
    .bind(grade_result.feedback.as_deref())
    .execute(&mut *tx)
    .await?;

    let (current_streak,): (i32,) =
        sqlx::query_as("SELECT advanced_correct_streak FROM journey_concepts WHERE journey_id = $1 AND concept_id = $2")
            .bind(journey_id)
            .bind(concept_id)
            .fetch_one(&mut *tx)
            .await?;

    let difficulty = pending.difficulty_attempted.as_deref().unwrap_or("");
    // PRD.md, Wrong Answer Handling [LOCKED]: any wrong answer (any
    // difficulty) resets the streak; only an ADVANCED correct answer
    // increments it; basic/intermediate correct leave it unchanged
    // (never explicitly mentioned in the locked rules, so not touched).
    let new_streak = if !grade_result.is_correct {
        0
    } else if difficulty == "advanced" {
        current_streak + 1
    } else {
        current_streak
    };

    // PRD.md, Mastery Score Formula [LOCKED]: new = alpha*old + beta*latest,
    // capped [0,1]. old defaults to 0.0 (no row yet = never assessed).
    let old_mastery: Option<f32> =
        sqlx::query_scalar("SELECT mastery_score FROM mastery_bank WHERE user_id = $1 AND concept_id = $2")
            .bind(user_id)
            .bind(concept_id)
            .fetch_optional(&mut *tx)
            .await?;
    let new_mastery =
        (state.mastery_alpha * old_mastery.unwrap_or(0.0) + state.mastery_beta * grade_result.score).clamp(0.0, 1.0);

    // PRD.md, Concept Completion Rule [LOCKED]: mastery_score >= 0.75 AND
    // advanced_correct_streak >= 3. is_complete never auto-reverts
    // (locked) — OR'd against whatever it already was, never unset.
    let now_complete = new_mastery >= state.mastery_completion_threshold && new_streak >= state.advanced_streak_required;

    sqlx::query(
        "INSERT INTO mastery_bank (user_id, concept_id, mastery_score, is_complete, total_attempts, last_assessed_at) \
         VALUES ($1, $2, $3, $4, 1, $5) \
         ON CONFLICT (user_id, concept_id) DO UPDATE SET \
           mastery_score = $3, is_complete = mastery_bank.is_complete OR $4, \
           total_attempts = mastery_bank.total_attempts + 1, last_assessed_at = $5",
    )
    .bind(user_id)
    .bind(concept_id)
    .bind(new_mastery)
    .bind(now_complete)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;

    if now_complete {
        sqlx::query(
            "UPDATE journey_concepts SET advanced_correct_streak = $3, attempt_count = attempt_count + 1, \
             last_attempt_at = $4, status = 'complete' WHERE journey_id = $1 AND concept_id = $2",
        )
        .bind(journey_id)
        .bind(concept_id)
        .bind(new_streak)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;

        // Advancement: any concept whose ENTIRE prerequisite set is now
        // 'complete' unlocks. Scoped to direct dependents of the concept
        // that just completed (not a full graph re-scan) — a concept
        // that was already unlockable before this attempt was already
        // unlocked by whichever earlier completion made it so.
        let dependents: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT concept_id FROM journey_prerequisites WHERE journey_id = $1 AND prereq_concept_id = $2",
        )
        .bind(journey_id)
        .bind(concept_id)
        .fetch_all(&mut *tx)
        .await?;

        for (dependent_id,) in dependents {
            let (total, satisfied): (i64, i64) = sqlx::query_as(
                "SELECT COUNT(*), COUNT(*) FILTER (WHERE jc.status = 'complete') \
                 FROM journey_prerequisites jp \
                 JOIN journey_concepts jc ON jc.journey_id = jp.journey_id AND jc.concept_id = jp.prereq_concept_id \
                 WHERE jp.journey_id = $1 AND jp.concept_id = $2",
            )
            .bind(journey_id)
            .bind(dependent_id)
            .fetch_one(&mut *tx)
            .await?;

            if total > 0 && total == satisfied {
                sqlx::query(
                    "UPDATE journey_concepts SET status = 'available' \
                     WHERE journey_id = $1 AND concept_id = $2 AND status = 'locked'",
                )
                .bind(journey_id)
                .bind(dependent_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    } else {
        sqlx::query(
            "UPDATE journey_concepts SET advanced_correct_streak = $3, attempt_count = attempt_count + 1, \
             last_attempt_at = $4 WHERE journey_id = $1 AND concept_id = $2",
        )
        .bind(journey_id)
        .bind(concept_id)
        .bind(new_streak)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Tutor's reaction — same "system prompt + history only, no
    // retrieval" shape as respond_to_confirmed_branch above: there's no
    // open academic question here to retrieve context for, just an
    // answer to react to. Grading is already final and deterministic by
    // this point (Rule 2) — this prompt only ever asks qwen to EXPLAIN/
    // ENCOURAGE, never to judge correctness itself.
    let thread_id = pending.thread_id;
    let outcome_word = if grade_result.is_correct { "correct" } else { "incorrect" };
    let feedback_clause = grade_result
        .feedback
        .as_deref()
        .map(|f| format!(" Grader feedback: {f}."))
        .unwrap_or_default();
    let reaction_hint = if grade_result.is_correct {
        "briefly affirm and encourage them onward"
    } else {
        "explain what likely went wrong and encourage another attempt"
    };
    let prompt = format!(
        "(The student just answered an exercise.\nQuestion: \"{}\"\nTheir answer: \"{student_answer}\"\n\
         Result: {outcome_word}.{feedback_clause}\nReact to this in your reply — {reaction_hint}. \
         Do not repeat the question verbatim.)",
        pending.rendered_question,
    );

    let history = load_recent_history(&state.pool, user_id, thread_id).await?;
    let chunks = ai_client::generate_stream(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        think,
        history,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<TurnEvent>(16);
    let state = state.clone();

    tokio::spawn(async move {
        let (accumulated, cutoff_reason) = stream_to_completion(chunks, &tx).await;
        let now = Utc::now();

        if let Err(err) = persist_turn(
            &state.pool,
            user_id,
            thread_id,
            now,
            &student_answer,
            &student_answer,
            &[],
            "exercise_answer",
            &accumulated,
            cutoff_reason.as_deref(),
        )
        .await
        {
            tracing::error!(?err, %journey_id, %thread_id, %attempt_id, "failed to persist exercise-answer turn");
        }
    });

    Ok(rx)
}

/// POST /journeys/{journey_id}/messages — every turn after the opening.
pub async fn send_journey_message(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
    raw_input: String,
    think: bool,
) -> Result<mpsc::Receiver<TurnEvent>, JourneyError> {
    let thread_id = find_thread(&state.pool, user_id, journey_id)
        .await?
        .ok_or_else(|| JourneyError::Validation("this journey's teaching thread hasn't started yet".to_string()))?;

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let (current_concept_id, pending_branch_topic): (Uuid, Option<String>) = sqlx::query_as(
        "SELECT current_concept_id, pending_branch_topic FROM study_threads WHERE thread_id = $1",
    )
    .bind(thread_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    // deferred.md #2d — the confirm-first branch offer: if last turn
    // asked "want a separate track for this?", THIS turn answers it
    // instead of being a normal message. Declined/ambiguous/failed
    // creation all fall through to normal turn processing below,
    // deliberately silent (no "ok, never mind" framing) — a scope cut,
    // see #2d's own deferred.md entry.
    if let Some(topic) = pending_branch_topic {
        clear_pending_branch_topic(&state.pool, user_id, thread_id).await?;
        if is_branch_confirmed(&raw_input) {
            // Found live (V3's audit list, bug #5): this call site skipped
            // journey_start_rate_limit entirely — the only other caller of
            // service::start() (handlers::start) checks it first. Same
            // check, same placement, matching that path exactly.
            let mut conn = state.get_redis_connection().await?;
            let (max, window) = state.journey_start_rate_limit;
            if !rate_limit::check_and_increment(&mut conn, &rate_limit::journey_start_key(user_id), max, window)
                .await?
            {
                return Err(JourneyError::RateLimited);
            }
            match super::service::start(state, user_id, topic.clone(), None, None, None).await {
                Ok(super::service::StartOutcome::JourneyCreated { .. }) => {
                    return respond_to_confirmed_branch(state, user_id, journey_id, thread_id, &topic, raw_input, think)
                        .await;
                }
                Ok(super::service::StartOutcome::Diagnostic { .. }) => {
                    // Unreachable in practice: level/goal are both None
                    // above, and start()'s own skip logic (level.is_none()
                    // || goal.is_none()) always takes the skip branch when
                    // that's true — defensive only.
                    tracing::error!(%journey_id, %thread_id, "deferred.md #2d: start() unexpectedly returned Diagnostic for a skip-path call");
                }
                Err(err) => {
                    tracing::warn!(?err, %journey_id, %thread_id, "deferred.md #2d: failed to create branched journey, continuing as a normal turn");
                }
            }
        }
    }

    let subject_id = verify_journey_and_subject(&state.pool, user_id, journey_id).await?;
    let (subject_title, dag_version): (String, i32) =
        sqlx::query_as("SELECT title, dag_version FROM subjects WHERE subject_id = $1")
            .bind(subject_id)
            .fetch_one(&state.pool)
            .await?;
    let known_terms = fetch_known_terms(&state.pool, subject_id, dag_version).await?;
    let current_concept_title = fetch_concept_title(&state.pool, current_concept_id).await?;

    let analysis = ai_client::analyze_input(
        &state.http_client,
        &state.ai_service_url,
        raw_input.clone(),
        // Cloned, not moved — the fold_gap trigger below needs its own
        // copies of subject_title/known_terms/current_concept_title
        // after this call consumes these ones.
        known_terms.clone(),
        Some(current_concept_id),
        // deferred.md #75/2b — both already fetched above for other
        // reasons (dag_version for known_terms itself), free here.
        Some(subject_id),
        Some(dag_version),
        // deferred.md #2b — same "already fetched above, free here"
        // reasoning; subject_title piggybacks on the dag_version query,
        // current_concept_title is one small additional lookup.
        Some(subject_title.clone()),
        Some(current_concept_title.clone()),
    )
    .await?;

    // deferred.md #2b/#2c/#2d — off_topic: no special handling (stays
    // plain TANGENT, same as before this whole classifier existed).
    // fold_gap: wired live — small, contained (this journey only, per
    // #2c's own decided dag_version scoping), matches #2e's "case
    // (b)-fold probably wants SOME acknowledgment" framing. branch_gap:
    // confirm-first (per the user's own explicit direction, 2026-08-11)
    // — records the offer, asks THIS turn, acts on the answer next turn
    // (see the pending_branch_topic block above).
    let mut extra_instruction: Option<String> = None;
    if let Some(gap) = &analysis.gap_classification {
        tracing::info!(%journey_id, %thread_id, gap_classification = %gap, "deferred.md #2b gap signal");
        if gap == "fold_gap" {
            match fold_gap_into_journey(
                state,
                user_id,
                journey_id,
                subject_id,
                dag_version,
                current_concept_id,
                &subject_title,
                &known_terms,
                &current_concept_title,
                &analysis.cleaned_query,
            )
            .await
            {
                Ok(title) => {
                    extra_instruction = Some(format!(
                        "(You just added a related concept, \"{title}\", to the student's learning path — \
                         briefly acknowledge this in your reply.)"
                    ));
                }
                Err(err) => {
                    tracing::warn!(?err, %journey_id, %thread_id, "deferred.md #2c: fold_gap failed, continuing without it");
                }
            }
        } else if gap == "branch_gap" {
            match set_pending_branch_topic(&state.pool, user_id, thread_id, &analysis.cleaned_query).await {
                Ok(()) => {
                    extra_instruction = Some(
                        "(What the student just raised seems like a substantial, separate topic from the \
                         current path — after addressing their message normally, ask if they'd like you to \
                         start a separate track for it. Don't start teaching that new topic here yet.)"
                            .to_string(),
                    );
                }
                Err(err) => {
                    tracing::warn!(?err, %journey_id, %thread_id, "deferred.md #2d: failed to record branch offer, continuing without it");
                }
            }
        }
    }

    // deferred.md — this journey's own uploaded content, always
    // included. See fetch_journey_upload_context's own doc comment.
    let journey_upload_context = match fetch_journey_upload_context(&state.pool, user_id, journey_id).await {
        Ok(context) => context,
        Err(err) => {
            tracing::warn!(?err, %journey_id, %thread_id, "failed to fetch this journey's own uploaded content, continuing without it");
            None
        }
    };

    // deferred.md #18: subject-scoped retrieval against the permanent
    // knowledge base — see query_knowledge_context's own doc comment.
    // Fails open exactly like memoryless/turn.rs's own equivalent call.
    let global_context = match query_knowledge_context(state, user_id, subject_id, journey_id, &analysis.cleaned_query).await {
        Ok(context) => context,
        Err(err) => {
            tracing::warn!(?err, %journey_id, %thread_id, "knowledge-base retrieval failed, continuing without it");
            None
        }
    };

    // deferred.md #91: journey_upload_context/global_context are real
    // reference material (student-uploaded or KB-retrieved — could be
    // any language, symbols, code) and go inside <reference_material>
    // tags so the model has an unambiguous target for _SYSTEM_PROMPT's
    // English-only check (see memoryless/turn.rs's identical fix for
    // the full story). extra_instruction is NOT reference material —
    // it's server-authored instruction text (V3's #2d branch-gap
    // acknowledgment), never user-provided content, so it's kept
    // outside both tag pairs, same as it already behaved.
    let mut reference_blocks: Vec<String> = Vec::new();
    if let Some(context) = &journey_upload_context {
        reference_blocks.push(format!(
            "The student uploaded this document to this conversation. Treat it as reference material \
             for what they're asking about:\n{context}"
        ));
    }
    if let Some(context) = &global_context {
        reference_blocks.push(format!(
            "Here is material from the knowledge base that may be relevant — use it if it helps, \
             but you don't have to:\n{context}"
        ));
    }

    let mut prompt_parts: Vec<String> = Vec::new();
    if !reference_blocks.is_empty() {
        prompt_parts.push(format!("<reference_material>\n{}\n</reference_material>", reference_blocks.join("\n\n")));
    }
    if let Some(instruction) = &extra_instruction {
        prompt_parts.push(instruction.clone());
    }

    let prompt = if prompt_parts.is_empty() {
        analysis.cleaned_query.clone()
    } else if reference_blocks.is_empty() {
        // extra_instruction only, no reference material — nothing for
        // the model to conflate the query with, so the old plain label
        // is enough (tags are reserved for the actual failure mode).
        format!("{}\n\nStudent's question: {}", prompt_parts.join("\n\n"), analysis.cleaned_query)
    } else {
        prompt_parts.push(format!("<student_message>\n{}\n</student_message>", analysis.cleaned_query));
        prompt_parts.join("\n\n")
    };

    let history = load_recent_history(&state.pool, user_id, thread_id).await?;

    let chunks = ai_client::generate_stream(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        think,
        history,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<TurnEvent>(16);
    let state = state.clone();
    let cleaned_query = analysis.cleaned_query;
    let matched_concepts = analysis.matched_concepts;
    let detected_intent = analysis.detected_intent;

    tokio::spawn(async move {
        let (accumulated, cutoff_reason) = stream_to_completion(chunks, &tx).await;
        let now = Utc::now();

        if let Err(err) = persist_turn(
            &state.pool,
            user_id,
            thread_id,
            now,
            &raw_input,
            &cleaned_query,
            &matched_concepts,
            &detected_intent,
            &accumulated,
            cutoff_reason.as_deref(),
        )
        .await
        {
            tracing::error!(?err, %journey_id, %thread_id, "failed to persist journey turn");
        }
    });

    Ok(rx)
}

#[allow(clippy::too_many_arguments)]
async fn persist_turn(
    pool: &PgPool,
    user_id: Uuid,
    thread_id: Uuid,
    now: chrono::DateTime<Utc>,
    raw_input: &str,
    cleaned_query: &str,
    matched_concepts: &[String],
    detected_intent: &str,
    response_text: &str,
    error: Option<&str>,
) -> Result<(), JourneyError> {
    let mut tx = begin_rls_transaction(pool, user_id).await?;

    sqlx::query("INSERT INTO messages (thread_id, role, content, mode, timestamp) VALUES ($1, 'user', $2, 'journey', $3)")
        .bind(thread_id)
        .bind(raw_input)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO messages (thread_id, role, content, mode, timestamp) VALUES ($1, 'tutor', $2, 'journey', $3)")
        .bind(thread_id)
        .bind(response_text)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE study_threads SET last_active_at = $2 WHERE thread_id = $1")
        .bind(thread_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;

    let matched_concepts_json = serde_json::json!(matched_concepts);
    sqlx::query(
        "INSERT INTO audit_logs \
         (thread_id, user_input, cleaned_query, matched_concepts, detected_intent, mode, response_text, model_used, error, timestamp) \
         VALUES ($1, $2, $3, $4, $5, 'journey', $6, $7, $8, $9)",
    )
    .bind(thread_id)
    .bind(raw_input)
    .bind(cleaned_query)
    .bind(matched_concepts_json)
    .bind(detected_intent)
    .bind(response_text)
    .bind(MODEL_USED)
    .bind(error)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// GET /journeys/{journey_id}/messages — hydration. None means no thread
/// exists yet (frontend should call /start instead).
pub async fn get_journey_thread(
    state: &AppState,
    user_id: Uuid,
    journey_id: Uuid,
) -> Result<Option<ThreadHydration>, JourneyError> {
    let Some(thread_id) = find_thread(&state.pool, user_id, journey_id).await? else {
        return Ok(None);
    };

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let (current_concept_title,): (String,) = sqlx::query_as(
        "SELECT cc.title FROM study_threads st \
         JOIN canonical_concepts cc ON cc.concept_id = st.current_concept_id \
         WHERE st.thread_id = $1",
    )
    .bind(thread_id)
    .fetch_one(&mut *tx)
    .await?;

    let rows: Vec<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT role, content, timestamp FROM messages WHERE thread_id = $1 ORDER BY timestamp ASC",
    )
    .bind(thread_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Some(ThreadHydration { thread_id, current_concept_title, messages: rows }))
}
