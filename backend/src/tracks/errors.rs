use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

// Same shape as content_flags::errors::ContentFlagError — the
// established convention every module's error type follows.
#[derive(Debug, thiserror::Error)]
pub enum TracksError {
    #[error("{0}")]
    Validation(String),
    // 404, not 403 — ensure_owns' own convention (Rule 34/IDOR): a
    // Track/journey that exists but belongs to someone else must look
    // identical to one that doesn't exist at all.
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal,
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for TracksError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            TracksError::Validation(_) => StatusCode::BAD_REQUEST,
            TracksError::NotFound => StatusCode::NOT_FOUND,
            TracksError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            TracksError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for TracksError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) {
            tracing::warn!(?err, "database unreachable in tracks handler");
            return TracksError::ServiceUnavailable("Database unreachable");
        }
        tracing::error!(?err, "database error in tracks handler");
        TracksError::Internal
    }
}

impl From<StatusCode> for TracksError {
    fn from(status: StatusCode) -> Self {
        // ensure_owns() only ever returns StatusCode::NOT_FOUND.
        debug_assert_eq!(status, StatusCode::NOT_FOUND);
        TracksError::NotFound
    }
}

impl From<crate::journeys::errors::JourneyError> for TracksError {
    fn from(err: crate::journeys::errors::JourneyError) -> Self {
        use crate::journeys::errors::JourneyError;
        match err {
            JourneyError::Validation(msg) => TracksError::Validation(msg),
            JourneyError::NotFound | JourneyError::DiagnosticExpiredOrNotFound => TracksError::NotFound,
            JourneyError::GenerationFailed(msg) => {
                tracing::error!(%msg, "journey thread generation failed during track creation");
                TracksError::Internal
            }
            JourneyError::ServiceUnavailable(msg) => TracksError::ServiceUnavailable(msg),
            // Unreachable in practice — create_journey_thread_sync never
            // calls the one rate-limited path (journeys::service::start);
            // mapped for match exhaustiveness only.
            JourneyError::Internal | JourneyError::RateLimited => TracksError::Internal,
        }
    }
}
