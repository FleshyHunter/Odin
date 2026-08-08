use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::state::AppState;

use super::errors::JourneyError;
use super::service;

#[derive(Deserialize)]
pub struct StartRequest {
    topic: String,
    level: String,
    goal: String,
    #[serde(default)]
    background: Option<String>,
}

#[derive(Serialize)]
pub struct StartResponse {
    diagnostic_id: Uuid,
    question: String,
    exercise_type: String,
    choices: Option<Vec<String>>,
}

pub async fn start(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Result<Json<StartResponse>, JourneyError> {
    let outcome = service::start(&state, user_id, req.topic, req.level, req.goal, req.background).await?;
    Ok(Json(StartResponse {
        diagnostic_id: outcome.diagnostic_id,
        question: outcome.question,
        exercise_type: outcome.exercise_type,
        choices: outcome.choices,
    }))
}

#[derive(Deserialize)]
pub struct RespondRequest {
    answer: String,
}

#[derive(Serialize)]
pub struct DiagnosticOutcomeResponse {
    contradicted: bool,
    backup_available: bool,
    journey_id: Option<Uuid>,
}

impl From<service::DiagnosticOutcome> for DiagnosticOutcomeResponse {
    fn from(outcome: service::DiagnosticOutcome) -> Self {
        Self {
            contradicted: outcome.contradicted,
            backup_available: outcome.backup_available,
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
