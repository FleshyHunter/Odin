// Block 5 builds the client; nothing calls embed() yet since there's no
// real feature needing embeddings until a much later block (ingestion/
// chunking) — verification of the actual live call is deferred too, per
// this pass's explicit scope (code only, not yet wired to a route).
#[allow(dead_code)]
mod ai_client;
mod auth;
mod config;
mod db;
mod models;
mod routes;
mod state;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use config::Config;
use state::AppState;

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

    let email_sender: Arc<dyn auth::email::EmailSender> = Arc::new(auth::email::ConsoleEmailSender);

    // 30s, not the 5s used for Redis/Postgres — ML inference (embedding
    // a batch of texts, later a full generation) is legitimately slower
    // than a connection ping, but this is still a bound, not "wait
    // forever": an unreachable/stalled FastAPI still fails clearly
    // rather than hanging (Rule 29's "no offline mode" philosophy).
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client");

    let app_state = AppState::new(pool, redis, &config, email_sender, http_client);

    let app = Router::new()
        .merge(routes::router())
        .merge(auth::router())
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("odin-backend listening on port {}", config.port);

    axum::serve(listener, app).await.expect("server error");
}
