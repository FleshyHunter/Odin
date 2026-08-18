use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
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
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };
    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
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
    current_concept_title: String,
    messages: Vec<JourneyMessageInfo>,
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
        current_concept_title: h.current_concept_title,
        messages: h
            .messages
            .into_iter()
            .map(|(role, content, timestamp)| JourneyMessageInfo { role, content, timestamp })
            .collect(),
    })))
}
