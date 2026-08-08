use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::ai_client::AiClientError;
use crate::exercises::errors::ExerciseError;
use crate::state::RedisConnectError;

#[derive(Debug, thiserror::Error)]
pub enum JourneyError {
    #[error("{0}")]
    Validation(String),
    // Distinct from NotFound for the same reason MemorylessError draws
    // this line (memoryless/errors.rs): a staged diagnostic that
    // genuinely expired vs. one that never existed are indistinguishable
    // on purpose (Rule 11) — both surface as 410, never 404.
    #[error("diagnostic expired or not found")]
    DiagnosticExpiredOrNotFound,
    #[error("not found")]
    NotFound,
    #[error("generation failed: {0}")]
    GenerationFailed(String),
    #[error("internal error")]
    Internal,
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for JourneyError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            JourneyError::Validation(_) => StatusCode::BAD_REQUEST,
            JourneyError::DiagnosticExpiredOrNotFound => StatusCode::GONE,
            JourneyError::NotFound => StatusCode::NOT_FOUND,
            JourneyError::GenerationFailed(_) => StatusCode::BAD_GATEWAY,
            JourneyError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            JourneyError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for JourneyError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) {
            tracing::warn!(?err, "database unreachable in journeys handler");
            return JourneyError::ServiceUnavailable("Database unreachable");
        }
        tracing::error!(?err, "database error in journeys handler");
        JourneyError::Internal
    }
}

impl From<redis::RedisError> for JourneyError {
    fn from(err: redis::RedisError) -> Self {
        if err.is_connection_refusal() || err.is_io_error() {
            tracing::warn!(?err, "redis unreachable in journeys handler");
            return JourneyError::ServiceUnavailable("Redis unreachable");
        }
        tracing::error!(?err, "redis error in journeys handler");
        JourneyError::Internal
    }
}

impl From<RedisConnectError> for JourneyError {
    fn from(err: RedisConnectError) -> Self {
        match err {
            RedisConnectError::Timeout => JourneyError::ServiceUnavailable("Redis unreachable"),
            RedisConnectError::Redis(e) => JourneyError::from(e),
        }
    }
}

impl From<serde_json::Error> for JourneyError {
    fn from(err: serde_json::Error) -> Self {
        tracing::error!(?err, "failed to (de)serialize staged onboarding diagnostic");
        JourneyError::Internal
    }
}

impl From<AiClientError> for JourneyError {
    fn from(err: AiClientError) -> Self {
        match err {
            AiClientError::ServiceUnavailable => JourneyError::ServiceUnavailable("AI service unreachable"),
            AiClientError::UnexpectedResponse(msg) => {
                tracing::error!(%msg, "unexpected AI service response in journeys handler");
                JourneyError::GenerationFailed(msg)
            }
        }
    }
}

// #13's lock reuses exercises::service::get_or_generate/fetch_one_canonical
// directly (an existing subject's diagnostic exercise is just another
// canonical exercise template) rather than duplicating that logic here.
impl From<ExerciseError> for JourneyError {
    fn from(err: ExerciseError) -> Self {
        match err {
            ExerciseError::GenerationFailed(msg) => JourneyError::GenerationFailed(msg),
            ExerciseError::Internal => JourneyError::Internal,
            ExerciseError::ServiceUnavailable(msg) => JourneyError::ServiceUnavailable(msg),
        }
    }
}
