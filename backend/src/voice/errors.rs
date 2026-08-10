use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::ai_client::AiClientError;

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("{0}")]
    Validation(String),
    #[error("internal error")]
    Internal,
    // Mirrors every other AI-gateway error enum's own reasoning (Rule
    // 29's "no offline mode... a clear failure is more honest than a
    // hang") — an unreachable/erroring ai_service fails this call
    // clearly rather than the composer just hanging on "transcribing…".
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for VoiceError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            VoiceError::Validation(_) => StatusCode::BAD_REQUEST,
            VoiceError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            VoiceError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<AiClientError> for VoiceError {
    fn from(err: AiClientError) -> Self {
        match err {
            AiClientError::ServiceUnavailable => VoiceError::ServiceUnavailable("AI service unreachable"),
            AiClientError::UnexpectedResponse(msg) => {
                tracing::error!(%msg, "unexpected AI service response in voice handler");
                VoiceError::Internal
            }
        }
    }
}
