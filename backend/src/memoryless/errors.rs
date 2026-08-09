use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use crate::ai_client::AiClientError;
use crate::journeys::errors::JourneyError;
use crate::state::RedisConnectError;

#[derive(Debug, thiserror::Error)]
pub enum MemorylessError {
    #[error("{0}")]
    Validation(String),
    // Distinct from a plain 404: this thread ID was once valid but its
    // Redis-staged entry has expired (or never existed) — the intended
    // hook for a future frontend "your session expired" message (PRD.md
    // Memoryless Mode). Also returned for a not-yet-converted thread_id
    // that simply never existed, since neither the client nor an
    // attacker should be able to tell those two cases apart.
    #[error("thread expired or not found")]
    ThreadExpiredOrNotFound,
    // Distinct from ThreadExpiredOrNotFound: this is an OWNERSHIP
    // mismatch (Rule 34's IDOR protection) — a thread_id that does
    // exist (possibly not even expired) but belongs to a different
    // user. Returning 404 rather than reusing 410 here matters: 410
    // would leak "yes, a thread once existed at this ID" to a caller
    // who has no business knowing that.
    #[error("not found")]
    NotFound,
    #[error("internal error")]
    Internal,
    // Mirrors AuthError::ServiceUnavailable's reasoning (Rule 29's "no
    // offline mode... a clear failure is more honest than a hang").
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for MemorylessError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            MemorylessError::Validation(_) => StatusCode::BAD_REQUEST,
            MemorylessError::ThreadExpiredOrNotFound => StatusCode::GONE,
            MemorylessError::NotFound => StatusCode::NOT_FOUND,
            MemorylessError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            MemorylessError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for MemorylessError {
    fn from(err: sqlx::Error) -> Self {
        // PoolTimedOut/Io mean Postgres itself is unreachable (found via
        // real testing during Block 12: a genuinely-down Postgres was
        // surfacing as a misleading 500 "internal error" instead of the
        // more honest 503 this already gets right for Redis, below) —
        // any other sqlx::Error is a real query/logic problem.
        if matches!(err, sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)) {
            tracing::warn!(?err, "database unreachable in memoryless handler");
            return MemorylessError::ServiceUnavailable("Database unreachable");
        }
        tracing::error!(?err, "database error in memoryless handler");
        MemorylessError::Internal
    }
}

impl From<redis::RedisError> for MemorylessError {
    fn from(err: redis::RedisError) -> Self {
        if err.is_connection_refusal() || err.is_io_error() {
            tracing::warn!(?err, "redis unreachable in memoryless handler");
            return MemorylessError::ServiceUnavailable("Redis unreachable");
        }
        tracing::error!(?err, "redis error in memoryless handler");
        MemorylessError::Internal
    }
}

impl From<RedisConnectError> for MemorylessError {
    fn from(err: RedisConnectError) -> Self {
        match err {
            RedisConnectError::Timeout => MemorylessError::ServiceUnavailable("Redis unreachable"),
            RedisConnectError::Redis(e) => MemorylessError::from(e),
        }
    }
}

impl From<serde_json::Error> for MemorylessError {
    fn from(err: serde_json::Error) -> Self {
        tracing::error!(?err, "failed to (de)serialize staged memoryless thread");
        MemorylessError::Internal
    }
}

impl From<AiClientError> for MemorylessError {
    fn from(err: AiClientError) -> Self {
        match err {
            AiClientError::ServiceUnavailable => {
                MemorylessError::ServiceUnavailable("AI service unreachable")
            }
            AiClientError::UnexpectedResponse(msg) => {
                tracing::error!(%msg, "unexpected AI service response in memoryless handler");
                MemorylessError::Internal
            }
        }
    }
}

// deferred.md #17: convert() reuses journeys::turn's
// verify_journey_and_subject/fetch_entry_concept directly, which return
// JourneyError — translated here rather than duplicating those queries.
impl From<JourneyError> for MemorylessError {
    fn from(err: JourneyError) -> Self {
        match err {
            JourneyError::Validation(msg) => MemorylessError::Validation(msg),
            // A journey that's expired-or-not-found or plain not-found are
            // both "this journey_id doesn't resolve for this user" here.
            JourneyError::DiagnosticExpiredOrNotFound | JourneyError::NotFound => MemorylessError::NotFound,
            JourneyError::GenerationFailed(msg) => {
                tracing::error!(%msg, "unexpected generation error resolving journey during conversion");
                MemorylessError::Internal
            }
            JourneyError::Internal => MemorylessError::Internal,
            JourneyError::ServiceUnavailable(msg) => MemorylessError::ServiceUnavailable(msg),
        }
    }
}
