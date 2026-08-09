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
