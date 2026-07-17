mod health;

use axum::{routing::get, Router};
use sqlx::PgPool;

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .with_state(pool)
}
