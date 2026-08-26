use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use redis::aio::MultiplexedConnection;
use sqlx::PgPool;

use crate::auth::email::EmailSender;
use crate::auth::errors::AuthError;
use crate::config::Config;
use crate::voice::session::VoiceSessionRegistry;

// Shared across every route now that auth needs Postgres + Redis + JWT
// signing + an email sender, not just the bare pool Block 1/2 needed.
// Clone is cheap: PgPool and redis::Client are both internally
// Arc-based/handle types, not deep copies.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    // A plain Client, not a pre-built ConnectionManager — Client::open
    // only parses the URL, it never touches the network, so
    // constructing AppState can never block/panic on Redis being
    // unreachable (ConnectionManager tries to eagerly connect and
    // panicked the whole process after a long timeout when Redis
    // wasn't up — same class of problem the Postgres pool had in
    // Block 1, fixed the same way: push the connection attempt to
    // per-request time, bounded by a short timeout, see get_redis()).
    redis_client: redis::Client,
    pub jwt_secret: Arc<str>,
    pub email_sender: Arc<dyn EmailSender>,
    pub access_token_expiry_minutes: i64,
    pub refresh_token_expiry_days: i64,
    pub otp_expiry_minutes: i64,
    pub verified_signup_token_ttl_minutes: i64,
    pub password_min_length: usize,
    // Refresh cookie's Secure attribute — see config.rs's own comment.
    pub cookie_secure: bool,
    // Block 5: shared across every ai_client call — reqwest::Client
    // pools connections internally, so this is built once here rather
    // than per-call, same reasoning as reusing the Postgres pool. Read
    // by the memoryless chat-turn handler (Block 11) via ai_client::
    // analyze_input/generate.
    pub http_client: reqwest::Client,
    // Independent-audit finding (2026-08-05, reopens deferred.md #20):
    // http_client's own `.timeout()` is reqwest's TOTAL-request deadline
    // (confirmed by reading reqwest 0.13.4's actual source —
    // async_impl/client.rs's own doc comment: "applied from when the
    // request starts connecting until the response body has finished")
    // — an absolute wall-clock cap, NOT reset per chunk. #20 believed
    // streaming had turned this into a per-chunk inactivity timeout, but
    // that's only true of turn.rs's OWN CHUNK_INACTIVITY_TIMEOUT
    // (a tokio::time::timeout wrapping chunks.next()) — the underlying
    // HTTP layer was never changed, so any healthy generation running
    // past http_client's 120s still gets killed mid-stream regardless
    // of continuous output. This second client uses `.read_timeout()`
    // instead — reqwest's own doc comment: "applies to each read
    // operation, and resets after a successful read... more appropriate
    // for detecting stalled connections when the size isn't known
    // beforehand" — the actually-correct primitive for a stream, used by
    // ai_client::generate_stream()/generate_stream_with_tools() and,
    // deliberately, by voice/handlers.rs's transcribe() too (see its own
    // comment). Every other ai_client call (analyze_input/embed/etc.)
    // keeps using http_client's total-deadline semantics, which are
    // correct for a single blocking response. deferred.md #102: its
    // read_timeout is 300s, not 120s — see main.rs's own comment on this
    // client for why.
    pub streaming_http_client: reqwest::Client,
    pub ai_service_url: Arc<str>,
    // Block 11: sliding TTL for Redis-staged memoryless threads.
    pub memoryless_thread_ttl_minutes: i64,
    // Size guardrail on staged uploads — see config.rs's own comment.
    pub memoryless_staged_upload_max_mb: u64,
    pub memoryless_staged_upload_max_chunks: usize,
    // generate_exercise_template()'s race-prevention lock TTL — see
    // exercises/service.rs.
    pub template_gen_lock_ttl_seconds: u64,
    // Redis Phase 1 (deferred.md #9-11) — see config.rs's own comments.
    pub login_rate_limit: (u32, u64),
    pub otp_resend_rate_limit: (u32, u64),
    pub signup_rate_limit: (u32, u64),
    pub password_reset_rate_limit: (u32, u64),
    pub otp_verify_attempt_limit: u32,
    // deferred.md #4: TTL for Redis-staged onboarding diagnostics — see
    // config.rs's own comment.
    pub onboarding_diagnostic_ttl_minutes: i64,
    // deferred.md #78 — see config.rs's own comment.
    pub journey_start_rate_limit: (u32, u64),
    pub journey_message_rate_limit: (u32, u64),
    pub memoryless_message_rate_limit: (u32, u64),
    // PRD.md Mastery System — see config.rs's own comment.
    pub mastery_alpha: f32,
    pub mastery_beta: f32,
    pub mastery_completion_threshold: f32,
    pub advanced_streak_required: i32,
    // deferred.md #80 follow-up — chunked voice-streaming transcription.
    // In-memory only, no persistence needed: session_id -> channel
    // handle for delivering partial-transcript results, torn down the
    // instant its SSE stream ends (see voice::handlers::stream_start's
    // own Drop guard). No constructor argument — always starts empty.
    pub voice_sessions: VoiceSessionRegistry,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        redis_client: redis::Client,
        config: &Config,
        email_sender: Arc<dyn EmailSender>,
        http_client: reqwest::Client,
        streaming_http_client: reqwest::Client,
    ) -> Self {
        Self {
            pool,
            redis_client,
            jwt_secret: Arc::from(config.jwt_secret.as_str()),
            email_sender,
            access_token_expiry_minutes: config.access_token_expiry_minutes,
            refresh_token_expiry_days: config.refresh_token_expiry_days,
            otp_expiry_minutes: config.otp_expiry_minutes,
            verified_signup_token_ttl_minutes: config.verified_signup_token_ttl_minutes,
            password_min_length: config.password_min_length,
            cookie_secure: config.cookie_secure,
            http_client,
            streaming_http_client,
            ai_service_url: Arc::from(config.ai_service_url.as_str()),
            memoryless_thread_ttl_minutes: config.memoryless_thread_ttl_minutes,
            memoryless_staged_upload_max_mb: config.memoryless_staged_upload_max_mb,
            memoryless_staged_upload_max_chunks: config.memoryless_staged_upload_max_chunks,
            template_gen_lock_ttl_seconds: config.template_gen_lock_ttl_seconds,
            login_rate_limit: config.login_rate_limit,
            otp_resend_rate_limit: config.otp_resend_rate_limit,
            signup_rate_limit: config.signup_rate_limit,
            password_reset_rate_limit: config.password_reset_rate_limit,
            otp_verify_attempt_limit: config.otp_verify_attempt_limit,
            onboarding_diagnostic_ttl_minutes: config.onboarding_diagnostic_ttl_minutes,
            journey_start_rate_limit: config.journey_start_rate_limit,
            journey_message_rate_limit: config.journey_message_rate_limit,
            memoryless_message_rate_limit: config.memoryless_message_rate_limit,
            mastery_alpha: config.mastery_alpha,
            mastery_beta: config.mastery_beta,
            mastery_completion_threshold: config.mastery_completion_threshold,
            advanced_streak_required: config.advanced_streak_required,
            voice_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// A fresh connection per call, bounded to 5s so an unreachable
    /// Redis fails a single request clearly instead of hanging or,
    /// worse, taking the whole process down with it. Error type is
    /// generic (not AuthError) so non-auth callers (Block 11's
    /// memoryless module) can map it to their own error enum via
    /// `From<RedisConnectError>` rather than borrowing auth's type.
    pub async fn get_redis_connection(&self) -> Result<MultiplexedConnection, RedisConnectError> {
        tokio::time::timeout(Duration::from_secs(5), self.redis_client.get_multiplexed_async_connection())
            .await
            .map_err(|_| RedisConnectError::Timeout)?
            .map_err(RedisConnectError::Redis)
    }

    pub async fn get_redis(&self) -> Result<MultiplexedConnection, AuthError> {
        self.get_redis_connection().await.map_err(AuthError::from)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RedisConnectError {
    #[error("Redis connection timed out")]
    Timeout,
    #[error(transparent)]
    Redis(#[from] redis::RedisError),
}

impl From<RedisConnectError> for AuthError {
    fn from(err: RedisConnectError) -> Self {
        match err {
            RedisConnectError::Timeout => AuthError::ServiceUnavailable("Redis unreachable"),
            RedisConnectError::Redis(e) => AuthError::from(e),
        }
    }
}
