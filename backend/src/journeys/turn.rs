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
async fn query_knowledge_context(
    state: &AppState,
    user_id: Uuid,
    subject_id: Uuid,
    query: &str,
) -> Result<Option<String>, AiClientError> {
    let query_embedding = ai_client::embed(&state.http_client, &state.ai_service_url, vec![query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AiClientError::UnexpectedResponse("embed() returned no vectors for the query".to_string()))?;

    knowledge::query_global_context(state, user_id, &query_embedding, Some(subject_id)).await
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
    let current_concept_id: Uuid =
        sqlx::query_scalar("SELECT current_concept_id FROM study_threads WHERE thread_id = $1")
            .bind(thread_id)
            .fetch_one(&mut *tx)
            .await?;
    tx.commit().await?;

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

    // deferred.md #2b/#2c — off_topic/branch_gap: observability only for
    // now (2d's trigger-UX question is still open, see its own entry —
    // creating a whole new journey unprompted has a much bigger blast
    // radius than folding one concept in). fold_gap: wired live below —
    // small, contained (this journey only, per #2c's own decided
    // dag_version scoping), matches #2e's own "case (b)-fold probably
    // wants SOME acknowledgment" framing.
    let mut fold_acknowledgment: Option<String> = None;
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
                Ok(title) => fold_acknowledgment = Some(title),
                Err(err) => {
                    tracing::warn!(?err, %journey_id, %thread_id, "deferred.md #2c: fold_gap failed, continuing without it");
                }
            }
        }
    }

    // deferred.md #18: subject-scoped retrieval against the permanent
    // knowledge base — see query_knowledge_context's own doc comment.
    // Fails open exactly like memoryless/turn.rs's own equivalent call.
    let global_context = match query_knowledge_context(state, user_id, subject_id, &analysis.cleaned_query).await {
        Ok(context) => context,
        Err(err) => {
            tracing::warn!(?err, %journey_id, %thread_id, "knowledge-base retrieval failed, continuing without it");
            None
        }
    };
    let prompt = match (&global_context, &fold_acknowledgment) {
        (Some(context), Some(title)) => format!(
            "Here is material from the knowledge base that may be relevant — use it if it helps, \
             but you don't have to:\n{context}\n\n(You just added a related concept, \"{title}\", to the \
             student's learning path — briefly acknowledge this in your reply.)\n\nStudent's question: {}",
            analysis.cleaned_query
        ),
        (Some(context), None) => format!(
            "Here is material from the knowledge base that may be relevant — use it if it helps, \
             but you don't have to:\n{context}\n\nStudent's question: {}",
            analysis.cleaned_query
        ),
        (None, Some(title)) => format!(
            "(You just added a related concept, \"{title}\", to the student's learning path — briefly \
             acknowledge this in your reply.)\n\nStudent's question: {}",
            analysis.cleaned_query
        ),
        (None, None) => analysis.cleaned_query.clone(),
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
