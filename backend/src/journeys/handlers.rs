use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{begin_rls_transaction, AuthUser};
use crate::auth::rate_limit;
use crate::state::AppState;

use super::errors::JourneyError;
use super::service;
use super::turn;

#[derive(Deserialize)]
pub struct StartRequest {
    topic: String,
    // PRD.md Step 1's skip mechanism ("skip diagnostic... trust the
    // self-report or absence of one as-is") — both absent together means
    // skip; StartRequest doesn't validate one-without-the-other, service::
    // start() treats "either missing" the same as "both missing" (see its
    // own doc comment).
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    background: Option<String>,
}

#[derive(Serialize)]
pub struct StartResponse {
    // Contract: EITHER journey_id is set (skip path — a journey was
    // created immediately, no diagnostic needed) OR the diagnostic_id/
    // question/exercise_type fields are set (normal path — answer this
    // question via /respond). Never both, never neither.
    diagnostic_id: Option<Uuid>,
    question: Option<String>,
    exercise_type: Option<String>,
    choices: Option<Vec<String>>,
    journey_id: Option<Uuid>,
}

impl From<service::StartOutcome> for StartResponse {
    fn from(outcome: service::StartOutcome) -> Self {
        match outcome {
            service::StartOutcome::Diagnostic { diagnostic_id, question, exercise_type, choices } => Self {
                diagnostic_id: Some(diagnostic_id),
                question: Some(question),
                exercise_type: Some(exercise_type),
                choices,
                journey_id: None,
            },
            service::StartOutcome::JourneyCreated { journey_id } => Self {
                diagnostic_id: None,
                question: None,
                exercise_type: None,
                choices: None,
                journey_id: Some(journey_id),
            },
        }
    }
}

pub async fn start(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Result<Json<StartResponse>, JourneyError> {
    // deferred.md #78: this triggers a real generate_dag() call (via
    // #13's own lock) — rate-limited before any of that real work
    // starts, same "check first" placement as the auth endpoints.
    let mut conn = state.get_redis_connection().await?;
    let (max, window) = state.journey_start_rate_limit;
    if !rate_limit::check_and_increment(&mut conn, &rate_limit::journey_start_key(user_id), max, window).await? {
        return Err(JourneyError::RateLimited);
    }

    let outcome = service::start(&state, user_id, req.topic, req.level, req.goal, req.background).await?;
    Ok(Json(outcome.into()))
}

#[derive(Deserialize)]
pub struct RespondRequest {
    answer: String,
}

#[derive(Serialize)]
pub struct BackupQuestionInfo {
    question: String,
    exercise_type: String,
    choices: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct DiagnosticOutcomeResponse {
    contradicted: bool,
    // Replaces a bare `backup_available: bool` — the frontend can't
    // render "try the backup question" without seeing what it actually
    // asks. Some(...) only when contradicted AND a backup exists;
    // retry_backup/confirm_downgrade's responses always leave this None
    // (a backup is never offered twice).
    backup_question: Option<BackupQuestionInfo>,
    journey_id: Option<Uuid>,
}

impl From<service::DiagnosticOutcome> for DiagnosticOutcomeResponse {
    fn from(outcome: service::DiagnosticOutcome) -> Self {
        Self {
            contradicted: outcome.contradicted,
            backup_question: outcome.backup_question.map(|q| BackupQuestionInfo {
                question: q.question,
                exercise_type: q.exercise_type,
                choices: q.choices,
            }),
            journey_id: outcome.journey_id,
        }
    }
}

pub async fn respond(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(diagnostic_id): Path<Uuid>,
    Json(req): Json<RespondRequest>,
) -> Result<Json<DiagnosticOutcomeResponse>, JourneyError> {
    let outcome = service::respond(&state, user_id, diagnostic_id, req.answer).await?;
    Ok(Json(outcome.into()))
}

pub async fn confirm_downgrade(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(diagnostic_id): Path<Uuid>,
) -> Result<Json<DiagnosticOutcomeResponse>, JourneyError> {
    let outcome = service::confirm_downgrade(&state, user_id, diagnostic_id).await?;
    Ok(Json(outcome.into()))
}

pub async fn retry_backup(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(diagnostic_id): Path<Uuid>,
    Json(req): Json<RespondRequest>,
) -> Result<Json<DiagnosticOutcomeResponse>, JourneyError> {
    let outcome = service::retry_backup(&state, user_id, diagnostic_id, req.answer).await?;
    Ok(Json(outcome.into()))
}

// deferred.md #2a — the real journey-mode chat turn loop. Same SSE
// shape as memoryless (backend/src/memoryless/handlers.rs), think
// defaults to true for the same reason (deferred.md #20 point 1: user-
// controlled, not auto-decided by detected_intent).

#[derive(Deserialize, Default)]
pub struct StartThreadRequest {
    #[serde(default)]
    think: Option<bool>,
}

/// POST /journeys/{journey_id}/start — the tutor-initiated opening turn.
/// `event: thread` and `event: concept` both fire once, immediately,
/// before any delta — the only way the caller learns the real thread_id
/// (doesn't exist until this call creates it) and what's being taught.
pub async fn start_journey_thread(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(journey_id): Path<Uuid>,
    body: Option<Json<StartThreadRequest>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, JourneyError> {
    let think = body.and_then(|Json(b)| b.think).unwrap_or(true);
    let (thread_id, concept_title, mut rx) = turn::start_journey_thread(&state, user_id, journey_id, think).await?;

    let sse_stream = async_stream::stream! {
        yield Ok(Event::default().event("thread").data(thread_id.to_string()));
        yield Ok(Event::default().event("concept").data(concept_title));
        while let Some(event) = rx.recv().await {
            match event {
                turn::TurnEvent::Delta(text) => yield Ok(Event::default().event("delta").data(text)),
                turn::TurnEvent::Error(reason) => yield Ok(Event::default().event("error").data(reason)),
                // #1/#2: only submit_exercise_answer's own turn.rs
                // function ever constructs a Result event — unreachable
                // here, but TurnEvent is shared across all three SSE
                // handlers in this file, so match must stay exhaustive.
                turn::TurnEvent::ExerciseResult(_) => {
                    tracing::warn!("unexpected TurnEvent::ExerciseResult from start_journey_thread's stream");
                }
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

#[derive(Deserialize)]
pub struct SendJourneyMessageRequest {
    message: String,
    #[serde(default)]
    think: Option<bool>,
}

/// POST /journeys/{journey_id}/messages — every turn after the opening.
pub async fn send_journey_message(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(journey_id): Path<Uuid>,
    Json(req): Json<SendJourneyMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, JourneyError> {
    if req.message.trim().is_empty() {
        return Err(JourneyError::Validation("message must not be empty".to_string()));
    }

    // deferred.md #78: a real generate_stream() call against Ollama.
    let mut conn = state.get_redis_connection().await?;
    let (max, window) = state.journey_message_rate_limit;
    if !rate_limit::check_and_increment(&mut conn, &rate_limit::journey_message_key(user_id), max, window).await? {
        return Err(JourneyError::RateLimited);
    }

    let think = req.think.unwrap_or(true);
    let mut rx = turn::send_journey_message(&state, user_id, journey_id, req.message, think).await?;

    let sse_stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match event {
                turn::TurnEvent::Delta(text) => yield Ok(Event::default().event("delta").data(text)),
                turn::TurnEvent::Error(reason) => yield Ok(Event::default().event("error").data(reason)),
                turn::TurnEvent::ExerciseResult(_) => {
                    tracing::warn!("unexpected TurnEvent::ExerciseResult from send_journey_message's stream");
                }
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

#[derive(Deserialize)]
pub struct ServeExerciseRequest {
    difficulty: String,
}

#[derive(Serialize)]
pub struct ServedExerciseInfo {
    attempt_id: Uuid,
    exercise_type: String,
    difficulty: String,
    rendered_question: String,
    rendered_choices: Option<Vec<String>>,
}

/// POST /journeys/{journey_id}/concepts/{concept_id}/exercise — serve a
/// fresh instantiated question. No dedicated rate limit (unlike message-
/// sending/journey-start): this doesn't call Dify, only the cheap,
/// deterministic instantiate_exercise() — real abuse-guarding can follow
/// once there's an actual signal it's needed, matching this codebase's
/// own "measure before generalizing" posture (deferred.md #5) rather
/// than a guess.
pub async fn serve_exercise(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((journey_id, concept_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ServeExerciseRequest>,
) -> Result<Json<ServedExerciseInfo>, JourneyError> {
    let served = turn::serve_exercise(&state, user_id, journey_id, concept_id, req.difficulty).await?;
    Ok(Json(ServedExerciseInfo {
        attempt_id: served.attempt_id,
        exercise_type: served.exercise_type,
        difficulty: served.difficulty,
        rendered_question: served.rendered_question,
        rendered_choices: served.rendered_choices,
    }))
}

#[derive(Deserialize)]
pub struct SubmitExerciseAnswerRequest {
    answer: String,
    #[serde(default)]
    think: Option<bool>,
}

/// POST /journeys/{journey_id}/exercises/{attempt_id}/submit — grades
/// the answer and streams the tutor's reaction, same SSE shape as
/// send_journey_message. Rate-limited the same way: this triggers a
/// real generate_stream() call against Ollama, same class of action
/// journey_message_rate_limit already exists to bound (deferred.md #78).
pub async fn submit_exercise_answer(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((journey_id, attempt_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SubmitExerciseAnswerRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, JourneyError> {
    if req.answer.trim().is_empty() {
        return Err(JourneyError::Validation("answer must not be empty".to_string()));
    }

    let mut conn = state.get_redis_connection().await?;
    let (max, window) = state.journey_message_rate_limit;
    if !rate_limit::check_and_increment(&mut conn, &rate_limit::journey_message_key(user_id), max, window).await? {
        return Err(JourneyError::RateLimited);
    }

    let think = req.think.unwrap_or(true);
    let mut rx = turn::submit_exercise_answer(&state, user_id, journey_id, attempt_id, req.answer, think).await?;

    let sse_stream = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            match event {
                turn::TurnEvent::Delta(text) => yield Ok(Event::default().event("delta").data(text)),
                turn::TurnEvent::Error(reason) => yield Ok(Event::default().event("error").data(reason)),
                // #1/#2: the one real emitter. Always sent first, before
                // any Delta (turn::submit_exercise_answer constructs and
                // sends this before spawning the prose-streaming task) —
                // grading/mastery/streak/advancement are already fully
                // computed and persisted by this point (Rule 2:
                // deterministic, no LLM call in the grading path itself),
                // so there's no latency cost to sending it up front.
                turn::TurnEvent::ExerciseResult(result) => {
                    let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                    yield Ok(Event::default().event("result").data(json));
                }
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

#[derive(Serialize)]
pub struct MasteryStatusInfo {
    mastery_score: f32,
    is_complete: bool,
    total_attempts: i32,
}

/// GET /journeys/{journey_id}/concepts/{concept_id}/mastery — #1/#2:
/// mastery_bank had zero readers anywhere before this (only ever
/// written by submit_exercise_answer). journey_id is unused inside the
/// query itself — mastery is keyed on (user_id, concept_id) only, never
/// journey_id (ARCHITECTURE_LOCK.md Rule 14, same fact already used
/// throughout the Track-delete work) — kept in the URL purely for
/// consistency with every other .../concepts/{concept_id}/... route
/// here. concept_title deliberately isn't returned: every real caller
/// already has it from its own surrounding context (the concept/node
/// it's currently displaying), so no join against canonical_concepts
/// is needed just to hand back a value the caller already has.
pub async fn get_mastery_status(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((_journey_id, concept_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Option<MasteryStatusInfo>>, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let row: Option<(f32, bool, i32)> = sqlx::query_as(
        "SELECT mastery_score, is_complete, total_attempts FROM mastery_bank \
         WHERE user_id = $1 AND concept_id = $2",
    )
    .bind(user_id)
    .bind(concept_id)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(row.map(|(mastery_score, is_complete, total_attempts)| MasteryStatusInfo {
        mastery_score,
        is_complete,
        total_attempts,
    })))
}

#[derive(Serialize)]
pub struct AttemptInfo {
    attempt_id: Uuid,
    rendered_question: String,
    is_correct: Option<bool>,
    difficulty_attempted: Option<String>,
    timestamp: chrono::DateTime<chrono::Utc>,
}

/// GET /journeys/{journey_id}/concepts/{concept_id}/history — #1/#2:
/// quiz_attempts had zero readers anywhere before this either. Scoped
/// to THIS journey (unlike mastery above) since attempt history is a
/// real per-journey record, not a cross-journey aggregate.
/// student_answer IS NOT NULL excludes the not-yet-answered row
/// serve_exercise creates immediately on instantiation (same
/// convention submit_exercise_answer's own WHERE clause already uses
/// to find it) — an unanswered attempt isn't history yet.
pub async fn get_node_history(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((journey_id, concept_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<AttemptInfo>>, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let rows: Vec<(Uuid, String, Option<bool>, Option<String>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT qa.attempt_id, qa.rendered_question, qa.is_correct, qa.difficulty_attempted, qa.timestamp \
         FROM quiz_attempts qa \
         JOIN exercises e ON e.exercise_id = qa.exercise_id \
         WHERE qa.journey_id = $1 AND e.concept_id = $2 AND qa.student_answer IS NOT NULL \
         ORDER BY qa.timestamp DESC",
    )
    .bind(journey_id)
    .bind(concept_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(attempt_id, rendered_question, is_correct, difficulty_attempted, timestamp)| AttemptInfo {
                attempt_id,
                rendered_question,
                is_correct,
                difficulty_attempted,
                timestamp,
            })
            .collect(),
    ))
}

#[derive(Serialize)]
pub struct RoadmapNodeInfo {
    concept_id: Uuid,
    title: String,
    // Raw "locked"/"available"/"in_progress"/"complete" — matches this
    // codebase's established split (backend returns DB-shaped data,
    // frontend toX() mappers translate), same as every other api/*.ts
    // file already does.
    status: String,
    prerequisite_ids: Vec<Uuid>,
    // deferred.md #38 — journey_concepts.foundation_gap / kiv_flagged_at,
    // exposed here rather than via a separate KIV-list endpoint: the Map
    // already fetches every real journey_concepts row for this journey,
    // so the KIV tab reuses that SAME fetch (ActivePanel derives its
    // list from this response) instead of a second, duplicate query.
    foundation_gap: bool,
    kiv_flagged: bool,
    // None if mastery_bank has no row yet for this concept (never
    // attempted) — real 0.0 is a different, meaningful state (attempted,
    // scored zero) from "no data at all."
    mastery_score: Option<f32>,
}

/// GET /journeys/{journey_id}/roadmap — deferred.md #94: the Map tab's
/// real data, replacing `sampleData.ts`. Order is a topological walk
/// (`service::topological_order`), not `journey_concepts.order_index`
/// (proven unreliable for graph semantics — see that function's own doc
/// comment) — entry concept first, every other node exactly once.
pub async fn get_roadmap(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(journey_id): Path<Uuid>,
) -> Result<Json<Vec<RoadmapNodeInfo>>, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    // No row at all means this journey doesn't exist or isn't owned by
    // this user (RLS-filtered) — NotFound. A row that exists but whose
    // subject somehow has no entry_concept_id set is a genuine invariant
    // violation, not a not-found case — same distinction service.rs's
    // own `subject.entry_concept_id.ok_or(JourneyError::Internal)` draws
    // elsewhere.
    let subject_row: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT s.entry_concept_id FROM journeys j JOIN subjects s ON s.subject_id = j.subject_id WHERE j.journey_id = $1",
    )
    .bind(journey_id)
    .fetch_optional(&mut *tx)
    .await?;
    let entry_concept_id = match subject_row {
        None => return Err(JourneyError::NotFound),
        Some((entry,)) => entry.ok_or(JourneyError::Internal)?,
    };

    // deferred.md #38: LEFT JOIN mastery_bank — a concept never attempted
    // has no row there at all, not a real score. Explicit mb.user_id
    // predicate alongside RLS, matching get_mastery_status's own
    // established convention in this same file.
    #[allow(clippy::type_complexity)]
    let node_rows: Vec<(Uuid, String, String, bool, Option<chrono::DateTime<chrono::Utc>>, Option<f32>)> = sqlx::query_as(
        "SELECT jc.concept_id, cc.title, jc.status, jc.foundation_gap, jc.kiv_flagged_at, mb.mastery_score \
         FROM journey_concepts jc \
         JOIN canonical_concepts cc ON cc.concept_id = jc.concept_id \
         LEFT JOIN mastery_bank mb ON mb.user_id = $2 AND mb.concept_id = jc.concept_id \
         WHERE jc.journey_id = $1",
    )
    .bind(journey_id)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;

    let edge_rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT concept_id, prereq_concept_id FROM journey_prerequisites WHERE journey_id = $1",
    )
    .bind(journey_id)
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    struct NodeMeta {
        title: String,
        status: String,
        foundation_gap: bool,
        kiv_flagged: bool,
        mastery_score: Option<f32>,
    }

    let mut meta_by_id: std::collections::HashMap<Uuid, NodeMeta> = node_rows
        .into_iter()
        .map(|(id, title, status, foundation_gap, kiv_flagged_at, mastery_score)| {
            (
                id,
                NodeMeta { title, status, foundation_gap, kiv_flagged: kiv_flagged_at.is_some(), mastery_score },
            )
        })
        .collect();
    let mut prereqs_by_id: std::collections::HashMap<Uuid, Vec<Uuid>> = std::collections::HashMap::new();
    for &(concept_id, prereq_concept_id) in &edge_rows {
        prereqs_by_id.entry(concept_id).or_default().push(prereq_concept_id);
    }
    let nodes_for_order: Vec<(Uuid, String)> =
        meta_by_id.iter().map(|(id, meta)| (*id, meta.title.clone())).collect();

    let ordered_ids = service::topological_order(&nodes_for_order, &edge_rows, entry_concept_id);

    Ok(Json(
        ordered_ids
            .into_iter()
            .map(|id| {
                let meta = meta_by_id.remove(&id);
                RoadmapNodeInfo {
                    concept_id: id,
                    title: meta.as_ref().map(|m| m.title.clone()).unwrap_or_default(),
                    status: meta.as_ref().map(|m| m.status.clone()).unwrap_or_else(|| "locked".to_string()),
                    foundation_gap: meta.as_ref().map(|m| m.foundation_gap).unwrap_or(false),
                    kiv_flagged: meta.as_ref().map(|m| m.kiv_flagged).unwrap_or(false),
                    mastery_score: meta.as_ref().and_then(|m| m.mastery_score),
                    prerequisite_ids: prereqs_by_id.remove(&id).unwrap_or_default(),
                }
            })
            .collect(),
    ))
}

/// POST /journeys/{journey_id}/concepts/{concept_id}/skip — deferred.md
/// #38: the real trigger for `journey_concepts.kiv_flagged_at`, unified
/// across both of PRD.md's locked KIV Review conditions ("moves on from
/// a failed advanced question" — `ExerciseCard.tsx`'s "Move on for now",
/// shown after a wrong advanced attempt — or "skips a foundation_gap
/// concept" — `NodeDetail.tsx`'s "Skip" button). Both call sites hit
/// this same endpoint; which PRD.md condition actually applied is
/// implicit in whichever concept's `foundation_gap` was already set at
/// intake, not tracked separately here. No hard gating, same posture as
/// everywhere else in this module — skipping a concept that's already
/// complete, or already flagged, is a harmless no-op re-timestamp, not
/// an error.
pub async fn skip_concept(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((journey_id, concept_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query(
        "UPDATE journey_concepts SET kiv_flagged_at = NOW() WHERE journey_id = $1 AND concept_id = $2",
    )
    .bind(journey_id)
    .bind(concept_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        return Err(JourneyError::NotFound);
    }
    tx.commit().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct JourneyMessageInfo {
    role: String,
    content: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct JourneyThreadInfo {
    thread_id: Uuid,
    current_concept_id: Uuid,
    current_concept_title: String,
    messages: Vec<JourneyMessageInfo>,
}

/// DELETE /journeys/{journey_id} — soft delete only, mirroring
/// study_threads.deleted_at's own "hides, never hard-deletes" contract.
/// Deliberately does NOT touch study_threads/Track rows, subjects,
/// canonical_concepts, or mastery_bank — journeys and Tracks are
/// independent, deleting one must never cascade into the other.
pub async fn delete_journey(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(journey_id): Path<Uuid>,
) -> Result<axum::http::StatusCode, JourneyError> {
    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;
    let result = sqlx::query("UPDATE journeys SET deleted_at = NOW() WHERE journey_id = $1 AND deleted_at IS NULL")
        .bind(journey_id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(JourneyError::NotFound);
    }
    tx.commit().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// GET /journeys/{journey_id}/messages — hydration. `null` means no
/// thread exists yet; the frontend should call `/start` instead.
pub async fn get_journey_messages(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(journey_id): Path<Uuid>,
) -> Result<Json<Option<JourneyThreadInfo>>, JourneyError> {
    let hydration = turn::get_journey_thread(&state, user_id, journey_id).await?;
    Ok(Json(hydration.map(|h| JourneyThreadInfo {
        thread_id: h.thread_id,
        current_concept_id: h.current_concept_id,
        current_concept_title: h.current_concept_title,
        messages: h
            .messages
            .into_iter()
            .map(|(role, content, timestamp)| JourneyMessageInfo { role, content, timestamp })
            .collect(),
    })))
}
