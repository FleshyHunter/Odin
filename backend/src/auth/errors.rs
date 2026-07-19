use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("invalid or expired code")]
    InvalidOtp,
    #[error("verification required or expired — request a new code")]
    VerificationRequired,
    #[error("{0}")]
    Validation(String),
    #[error("an account with this email already exists")]
    AccountExists,
    #[error("invalid or expired reset token")]
    InvalidResetToken,
    #[error("invalid or expired refresh token")]
    InvalidRefreshToken,
    #[error("internal error")]
    Internal,
    // Distinct from Internal: this specifically means an external
    // dependency (Redis) was unreachable, matching Rule 29's "no
    // offline mode... accepted behavior, not a gap to engineer
    // around" — a clear 503 is more honest here than a generic 500.
    #[error("{0}")]
    ServiceUnavailable(&'static str),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            AuthError::InvalidCredentials
            | AuthError::InvalidOtp
            | AuthError::VerificationRequired
            | AuthError::InvalidResetToken
            | AuthError::InvalidRefreshToken => StatusCode::UNAUTHORIZED,
            AuthError::Validation(_) => StatusCode::BAD_REQUEST,
            AuthError::AccountExists => StatusCode::CONFLICT,
            AuthError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(?err, "database error in auth handler");
        AuthError::Internal
    }
}

impl From<redis::RedisError> for AuthError {
    fn from(err: redis::RedisError) -> Self {
        // Redis being unreachable (connection refused, I/O failure)
        // fails FAST — the OS rejects a closed port near-instantly, it
        // doesn't need the full 5s timeout in AppState::get_redis to
        // notice. That path was landing here as a generic Internal
        // (500) instead of the more honest ServiceUnavailable (503) —
        // only an actual timeout was mapped correctly before this fix.
        if err.is_connection_refusal() || err.is_io_error() {
            tracing::warn!(?err, "redis unreachable in auth handler");
            return AuthError::ServiceUnavailable("Redis unreachable");
        }
        tracing::error!(?err, "redis error in auth handler");
        AuthError::Internal
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        tracing::error!(?err, "jwt error in auth handler");
        AuthError::Internal
    }
}

impl From<argon2::password_hash::Error> for AuthError {
    fn from(err: argon2::password_hash::Error) -> Self {
        tracing::error!(?err, "password hashing error in auth handler");
        AuthError::Internal
    }
}
