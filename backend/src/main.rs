// Block 5 builds the client; nothing calls embed() yet since there's no
// real feature needing embeddings until a much later block (ingestion/
// chunking) — verification of the actual live call is deferred too, per
// this pass's explicit scope (code only, not yet wired to a route).
// analyze_input()/generate()/generate_stream() (memoryless chat turns)
// and generate_exercise_template()/ingest() (Block 12: exercises/,
// uploads/) are now used; embed()/transcribe()/acquire()/generate_dag()/
// grade() still have no Rust caller until later blocks wire ingestion/
// onboarding/grading routes, hence the module-level allow still covers
// the rest.
#[allow(dead_code)]
mod ai_client;
mod auth;
mod config;
mod content_flags;
mod db;
mod exercises;
mod memoryless;
mod models;
mod routes;
mod state;
mod uploads;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, HeaderValue, Method};
use axum::Router;
use config::Config;
use state::AppState;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Loads backend/.env in dev; in real deployment these come from the
    // actual environment, not a file — ok() so a missing file (e.g. in
    // production) isn't a hard error.
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    let pool = db::connect(&config.database_url, config.db_pool_size)
        .expect("failed to create Postgres pool");

    // Infallible in practice for any well-formed URL — see db::connect_redis.
    let redis = db::connect_redis(&config.redis_url).expect("invalid REDIS_URL");

    // Real SMTP delivery when all three are configured (see
    // auth/email.rs's own doc comment for why this is SMTP, not
    // Resend); falls back to the console-log stub otherwise, same
    // "optional, degrades to a stub" pattern as Dify's keys.
    let email_sender: Arc<dyn auth::email::EmailSender> =
        match (&config.smtp_relay, &config.smtp_username, &config.smtp_password) {
            (Some(relay), Some(username), Some(password)) => {
                let sender = auth::email::SmtpEmailSender::new(relay, username.clone(), password.clone())
                    .expect("invalid SMTP configuration");
                tracing::info!(%relay, %username, "SMTP email sender configured");
                Arc::new(sender)
            }
            _ => {
                tracing::warn!("SMTP not configured (SMTP_RELAY/SMTP_USERNAME/SMTP_PASSWORD) — using console-log stub for OTP/password-reset emails");
                Arc::new(auth::email::ConsoleEmailSender)
            }
        };

    // 120s, not the 5s used for Redis/Postgres — ML inference is
    // legitimately slower than a connection ping, and a reasoning model
    // (qwen3.5:9b, thinking on by default) can take real time even once
    // the chat-template/turn-boundary fix is in place. Still a bound,
    // not "wait forever": an unreachable/stalled FastAPI still fails
    // clearly rather than hanging (Rule 29's "no offline mode"
    // philosophy) — no streaming yet, so this has to cover a whole
    // response, not just an idle gap between chunks.
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client");

    // Browser-only same-origin gate — separate from and unrelated to the
    // JWT/RLS auth layers below. allow_credentials(true) is required so
    // the browser will actually attach/accept the httpOnly refresh_token
    // cookie cross-origin (5173 -> 8080 in local dev); that flag can't be
    // paired with a wildcard origin (tower-http rejects it at runtime),
    // so this is one explicit origin, not Any.
    let cors_origin: HeaderValue = config
        .frontend_origin
        .parse()
        .expect("FRONTEND_ORIGIN must be a valid header value (e.g. http://localhost:5173)");
    let cors = CorsLayer::new()
        .allow_origin(cors_origin)
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app_state = AppState::new(pool, redis, &config, email_sender, http_client);

    let app = Router::new()
        .merge(routes::router())
        .merge(auth::router())
        .merge(memoryless::router())
        .merge(uploads::router(config.memoryless_staged_upload_max_mb))
        .merge(exercises::router())
        .merge(content_flags::router())
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("odin-backend listening on port {}", config.port);

    axum::serve(listener, app).await.expect("server error");
}
