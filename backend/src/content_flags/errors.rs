use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ContentFlagError {
    #[error("{0}")]
    Validation(String),
    #[error("internal error")]
    Internal,
    // Distinct from Internal, matching the same fix already applied to
    // Auth/Memoryless/Exercise errors (deferred.md #28) — a genuinely
    // unreachable Postgres deserves a 503, not a generic 500. Built
    // this way from the start here, not retrofitted after the fact.
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for ContentFlagError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            ContentFlagError::Validation(_) => StatusCode::BAD_REQUEST,
            ContentFlagError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ContentFlagError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for ContentFlagError {
    fn from(err: sqlx::Error) -> Self {
        if matches!(err, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) {
            tracing::warn!(?err, "database unreachable in content_flags handler");
            return ContentFlagError::ServiceUnavailable("Database unreachable");
        }
        tracing::error!(?err, "database error in content_flags handler");
        ContentFlagError::Internal
    }
}
