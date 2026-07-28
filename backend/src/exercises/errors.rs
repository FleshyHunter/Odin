use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::ai_client::AiClientError;
use crate::state::RedisConnectError;

#[derive(Debug, thiserror::Error)]
pub enum ExerciseError {
    #[error("exercise generation failed: {0}")]
    GenerationFailed(String),
    #[error("internal error")]
    Internal,
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for ExerciseError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            ExerciseError::GenerationFailed(_) => StatusCode::BAD_GATEWAY,
            ExerciseError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ExerciseError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for ExerciseError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) {
            tracing::warn!(?err, "database unreachable in exercises handler");
            return ExerciseError::ServiceUnavailable("Database unreachable");
        }
        tracing::error!(?err, "database error in exercises handler");
        ExerciseError::Internal
    }
}

impl From<redis::RedisError> for ExerciseError {
    fn from(err: redis::RedisError) -> Self {
        if err.is_connection_refusal() || err.is_io_error() {
            tracing::warn!(?err, "redis unreachable in exercises handler");
            return ExerciseError::ServiceUnavailable("Redis unreachable");
        }
        tracing::error!(?err, "redis error in exercises handler");
        ExerciseError::Internal
    }
}

impl From<RedisConnectError> for ExerciseError {
    fn from(err: RedisConnectError) -> Self {
        match err {
            RedisConnectError::Timeout => ExerciseError::ServiceUnavailable("Redis unreachable"),
            RedisConnectError::Redis(e) => ExerciseError::from(e),
        }
    }
}

impl From<AiClientError> for ExerciseError {
    fn from(err: AiClientError) -> Self {
        match err {
            AiClientError::ServiceUnavailable => ExerciseError::ServiceUnavailable("AI service unreachable"),
            AiClientError::UnexpectedResponse(msg) => {
                tracing::error!(%msg, "unexpected AI service response in exercises handler");
                ExerciseError::GenerationFailed(msg)
            }
        }
    }
}
