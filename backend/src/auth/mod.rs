pub mod email;
pub mod errors;
mod handlers;
mod hash_utils;
pub mod jwt;
pub mod middleware;
mod otp;
mod password;
mod tokens;

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::state::AppState;
use middleware::AuthUser;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup/request-otp", post(handlers::signup_request_otp))
        .route("/auth/signup/verify-otp", post(handlers::signup_verify_otp))
        .route("/auth/signup/complete", post(handlers::signup_complete))
        .route("/auth/login/password", post(handlers::login_password))
        .route("/auth/login/request-otp", post(handlers::login_request_otp))
        .route("/auth/login/verify-otp", post(handlers::login_verify_otp))
        .route("/auth/refresh", post(handlers::refresh))
        .route("/auth/logout", post(handlers::logout))
        .route("/auth/password-reset/request", post(handlers::password_reset_request))
        .route("/auth/password-reset/confirm", post(handlers::password_reset_confirm))
        .route("/auth/me", get(me))
}

// A real, standard endpoint (the frontend needs "who am I" regardless)
// that also happens to be the cleanest proof the auth_guard + RLS
// transaction work together over real HTTP: this fetches the caller's
// own row through an RLS-scoped transaction, not a plain query — if
// either the JWT guard or the RLS policy were broken, this would 401
// or return no rows despite a real row existing (same proof as Block
// 2's psql test, now through the actual request path).
async fn me(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, errors::AuthError> {
    let mut tx = middleware::begin_rls_transaction(&state.pool, user_id).await?;
    let user: crate::models::auth::User = sqlx::query_as("SELECT * FROM users WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "user_id": user.user_id,
        "display_name": user.display_name,
        "email": user.email,
    })))
}
