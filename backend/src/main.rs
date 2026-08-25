// Block 5 builds the client; nothing calls embed() yet since there's no
// real feature needing embeddings until a much later block (ingestion/
// chunking) — verification of the actual live call is deferred too, per
// this pass's explicit scope (code only, not yet wired to a route).
// analyze_input()/generate()/generate_stream() (memoryless chat turns),
// generate_exercise_template()/ingest() (Block 12: exercises/, uploads/),
// generate_dag()/adjust_dag()/grade() (deferred.md #4: journeys::),
// embed() (deferred.md #18/#19: staged-upload + knowledge_global
// retrieval, both memoryless and journey chat turns), and transcribe()
// (deferred.md #80: voice::) are now used; acquire() still has no Rust
// caller (deferred.md #12) — the module-level allow still covers that one.
#[allow(dead_code)]
mod ai_client;
mod auth;
mod config;
mod content_flags;
mod db;
mod exercises;
mod journeys;
mod knowledge;
mod memoryless;
mod models;
mod routes;
mod state;
mod tracks;
mod uploads;
mod voice;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, HeaderValue, Method};
use axum::Router;
use config::Config;
use state::AppState;
use tower_http::cors::{AllowOrigin, CorsLayer};

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
                let sender = auth::email::SmtpEmailSender::new(
                    relay,
                    username.clone(),
                    password.clone(),
                    config.smtp_send_timeout_seconds,
                )
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
    // (qwen3.5:9b, thinking on by default) can take real time. Still a
    // bound, not "wait forever": an unreachable/stalled FastAPI still
    // fails clearly rather than hanging (Rule 29's "no offline mode"
    // philosophy). Used for every ai_client call EXCEPT generate_stream
    // (see streaming_http_client below) — a single blocking response is
    // exactly what reqwest's `.timeout()` (a TOTAL request deadline) is
    // for.
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build reqwest client");

    // Independent-audit finding (2026-08-05, reopens deferred.md #20):
    // `.timeout()` above is a TOTAL deadline from request start —
    // reqwest 0.13.4's own doc comment confirms it covers "from when the
    // request starts connecting until the response body has finished,"
    // not reset per chunk. #20 believed streaming turned this into a
    // per-chunk inactivity timeout; it didn't — only turn.rs's own
    // separate CHUNK_INACTIVITY_TIMEOUT (application-level, wrapping
    // chunks.next()) actually behaves that way. A healthy generation
    // streaming real deltas continuously past 120s total was still
    // getting killed by THIS client regardless. `.read_timeout()`
    // instead is reqwest's actual per-chunk-reset primitive (its own doc
    // comment: "resets after a successful read... appropriate for
    // detecting stalled connections when the size isn't known
    // beforehand") — used ONLY by generate_stream(), which already has
    // its own redundant, correct safety net at the application layer.
    let streaming_http_client = reqwest::Client::builder()
        .read_timeout(Duration::from_secs(120))
        .build()
        .expect("failed to build streaming reqwest client");

    // Browser-only same-origin gate — separate from and unrelated to the
    // JWT/RLS auth layers below. allow_credentials(true) is required so
    // the browser will actually attach/accept the httpOnly refresh_token
    // cookie cross-origin (5174 -> 8080 in local dev); that flag can't be
    // paired with a wildcard origin (tower-http rejects it at runtime),
    // so this stays an explicit allow-list, never Any.
    // FRONTEND_ORIGIN accepts a comma-separated list, not just one origin
    // — added so a phone/other device on the same LAN can reach the Vite
    // dev server (started with --host, see vite.config.ts) at the Mac's
    // real LAN IP while `localhost:5174` keeps working for the Mac itself.
    // Still every entry is a single, explicit, exact origin string — this
    // widens the allow-list, it doesn't loosen the match into a wildcard
    // or a pattern.
    let cors_origins: Vec<HeaderValue> = config
        .frontend_origin
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|origin| {
            origin.parse().unwrap_or_else(|_| {
                panic!(
                    "FRONTEND_ORIGIN entry {origin:?} is not a valid header value \
                     (e.g. http://localhost:5174)"
                )
            })
        })
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_origins))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app_state = AppState::new(pool, redis, &config, email_sender, http_client, streaming_http_client);

    let app = Router::new()
        .merge(routes::router())
        .merge(auth::router())
        .merge(memoryless::router())
        .merge(uploads::router(config.memoryless_staged_upload_max_mb))
        .merge(exercises::router())
        .merge(content_flags::router())
        .merge(journeys::router())
        .merge(tracks::router())
        .merge(voice::router())
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("odin-backend listening on port {}", config.port);

    // deferred.md #9-11: SIGNUP_RATE_LIMIT is per-IP (PRD.md NC7 — "3
    // signup attempts/hour/IP"), the one rate limit that isn't
    // email-keyed — needs the real client address, which requires this
    // connect-info-aware serve variant instead of the plain one.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server error");
}
