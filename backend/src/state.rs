use std::sync::Arc;
use std::time::Duration;

use redis::aio::MultiplexedConnection;
use sqlx::PgPool;

use crate::auth::email::EmailSender;
use crate::auth::errors::AuthError;
use crate::config::Config;

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
    // Block 5: shared across every ai_client call — reqwest::Client
    // pools connections internally, so this is built once here rather
    // than per-call, same reasoning as reusing the Postgres pool. Read
    // by the memoryless chat-turn handler (Block 11) via ai_client::
    // analyze_input/generate.
    pub http_client: reqwest::Client,
    pub ai_service_url: Arc<str>,
    // Block 11: sliding TTL for Redis-staged memoryless threads.
    pub memoryless_thread_ttl_minutes: i64,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        redis_client: redis::Client,
        config: &Config,
        email_sender: Arc<dyn EmailSender>,
        http_client: reqwest::Client,
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
            http_client,
            ai_service_url: Arc::from(config.ai_service_url.as_str()),
            memoryless_thread_ttl_minutes: config.memoryless_thread_ttl_minutes,
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
