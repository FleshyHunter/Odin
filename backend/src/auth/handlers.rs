use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use time::Duration as CookieDuration;
use uuid::Uuid;

use crate::models::auth::User;
use crate::state::AppState;

use super::errors::AuthError;
use super::hash_utils::sha256_hex;
use super::jwt::{self, TokenType};
use super::otp;
use super::password;
use super::tokens::issue_tokens;

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

#[derive(Serialize)]
pub struct UserPublic {
    user_id: Uuid,
    display_name: String,
    email: String,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self { user_id: u.user_id, display_name: u.display_name, email: u.email }
    }
}

#[derive(Serialize)]
pub struct AuthResponse {
    access_token: String,
    user: UserPublic,
}

fn refresh_cookie(token: &str, days: i64) -> Cookie<'static> {
    // Secure(true) omitted deliberately for now — this is plain HTTP in
    // local dev (see Rule 49's "Secure in production"); flip on once
    // this is ever actually served over HTTPS.
    Cookie::build(("refresh_token", token.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/auth/refresh")
        .max_age(CookieDuration::days(days))
        .build()
}

async fn respond_with_tokens(
    state: &AppState,
    user: User,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let issued = issue_tokens(
        &state.pool,
        user.user_id,
        &state.jwt_secret,
        state.access_token_expiry_minutes,
        state.refresh_token_expiry_days,
    )
    .await?;

    let jar = CookieJar::new().add(refresh_cookie(&issued.refresh_token, state.refresh_token_expiry_days));
    let response = AuthResponse { access_token: issued.access_token, user: user.into() };
    Ok((jar, Json(response)))
}

// ---------------- Signup (OTP-first, 3 steps) ----------------

#[derive(Deserialize)]
pub struct EmailRequest {
    email: String,
}

pub async fn signup_request_otp(
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;
    let code = otp::stage_signup_otp(&mut conn, &email, state.otp_expiry_minutes).await?;
    state.email_sender.send_otp(&email, &code).await;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct VerifyOtpRequest {
    email: String,
    code: String,
}

pub async fn signup_verify_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;
    let matched = otp::verify_signup_otp(&mut conn, &email, &req.code).await?;
    if !matched {
        return Err(AuthError::InvalidOtp);
    }
    otp::mark_signup_verified(&mut conn, &email, state.verified_signup_token_ttl_minutes).await?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct CompleteSignupRequest {
    email: String,
    display_name: String,
    password: String,
}

pub async fn signup_complete(
    State(state): State<AppState>,
    Json(req): Json<CompleteSignupRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;

    // Step 3 only proceeds if step 2 actually succeeded recently for
    // THIS email (ARCHITECTURE.md: "so a stale, long-abandoned tab
    // can't submit credentials off an OTP match from much earlier").
    if !otp::consume_signup_verified(&mut conn, &email).await? {
        return Err(AuthError::VerificationRequired);
    }

    password::validate_password(&req.password, state.password_min_length)
        .map_err(AuthError::Validation)?;
    let password_hash = password::hash_password(&req.password)?;

    // ONE single INSERT (Auth section) — no partial/pending row ever
    // exists before this. email_verified defaults TRUE at the schema
    // level (Rule 49) since this row is only ever created post-OTP.
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (display_name, email, email_normalized, password_hash)
         VALUES ($1, $2, $3, $4)
         RETURNING *",
    )
    .bind(&req.display_name)
    .bind(&req.email)
    .bind(&email)
    .bind(&password_hash)
    .fetch_one(&state.pool)
    .await
    .map_err(|err| match &err {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => AuthError::AccountExists,
        _ => AuthError::from(err),
    })?;

    respond_with_tokens(&state, user).await
}

// ---------------- Login (two permanent, equal methods) ----------------

#[derive(Deserialize)]
pub struct LoginPasswordRequest {
    email: String,
    password: String,
}

pub async fn login_password(
    State(state): State<AppState>,
    Json(req): Json<LoginPasswordRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let email = normalize_email(&req.email);
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email_normalized = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;

    let Some(user) = user else {
        return Err(AuthError::InvalidCredentials);
    };
    if !password::verify_password(&req.password, &user.password_hash) {
        return Err(AuthError::InvalidCredentials);
    }

    respond_with_tokens(&state, user).await
}

pub async fn login_request_otp(
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_normalized = $1)")
        .bind(&email)
        .fetch_one(&state.pool)
        .await?;

    // Always 200 regardless of whether the account exists — avoids
    // letting this endpoint be used to enumerate registered emails.
    // Only actually stages/sends a code when there's a real account.
    if exists {
        let mut conn = state.get_redis().await?;
        let code = otp::stage_login_otp(&mut conn, &email, state.otp_expiry_minutes).await?;
        state.email_sender.send_otp(&email, &code).await;
    }
    Ok(StatusCode::OK)
}

pub async fn login_verify_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;
    let matched = otp::verify_login_otp(&mut conn, &email, &req.code).await?;
    if !matched {
        return Err(AuthError::InvalidOtp);
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email_normalized = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    respond_with_tokens(&state, user).await
}

// ---------------- Token refresh / logout ----------------

#[derive(Serialize)]
pub struct RefreshResponse {
    access_token: String,
}

pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<RefreshResponse>, AuthError> {
    let raw_token = jar
        .get("refresh_token")
        .map(|c| c.value().to_string())
        .ok_or(AuthError::InvalidRefreshToken)?;

    let claims = jwt::verify(&raw_token, TokenType::Refresh, &state.jwt_secret)
        .map_err(|_| AuthError::InvalidRefreshToken)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidRefreshToken)?;

    let token_hash = sha256_hex(&raw_token);
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM refresh_tokens
            WHERE user_id = $1 AND token_hash = $2
              AND revoked_at IS NULL AND expires_at > NOW()
        )",
    )
    .bind(user_id)
    .bind(&token_hash)
    .fetch_one(&state.pool)
    .await?;

    if !valid {
        return Err(AuthError::InvalidRefreshToken);
    }

    let access_token = jwt::issue_access_token(user_id, state.access_token_expiry_minutes, &state.jwt_secret)?;
    Ok(Json(RefreshResponse { access_token }))
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<(CookieJar, StatusCode), AuthError> {
    if let Some(raw_token) = jar.get("refresh_token").map(|c| c.value().to_string()) {
        let token_hash = sha256_hex(&raw_token);
        sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND revoked_at IS NULL")
            .bind(&token_hash)
            .execute(&state.pool)
            .await?;
    }
    let cleared = jar.remove(Cookie::from("refresh_token"));
    Ok((cleared, StatusCode::NO_CONTENT))
}

// ---------------- Password reset ----------------

pub async fn password_reset_request(
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);

    // Same anti-enumeration shape as login_request_otp — always 200,
    // only actually issues a token when the account is real.
    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM users WHERE email_normalized = $1")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;

    if let Some(user_id) = user_id {
        let raw_token = Uuid::new_v4().to_string();
        let token_hash = sha256_hex(&raw_token);
        let expires_at = Utc::now() + chrono::Duration::hours(state.password_reset_token_expiry_hours);

        sqlx::query(
            "UPDATE users
             SET password_reset_token_hash = $1, password_reset_expires_at = $2, password_reset_used_at = NULL
             WHERE user_id = $3",
        )
        .bind(&token_hash)
        .bind(expires_at)
        .bind(user_id)
        .execute(&state.pool)
        .await?;

        // Real link shape (frontend route, real domain) is a later
        // concern — the token itself is the real artifact here.
        let reset_link = format!("https://odin.app/reset-password?email={email}&token={raw_token}");
        state.email_sender.send_password_reset(&email, &reset_link).await;
    }

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmRequest {
    email: String,
    token: String,
    new_password: String,
}

pub async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirmRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);
    password::validate_password(&req.new_password, state.password_min_length)
        .map_err(AuthError::Validation)?;

    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email_normalized = $1
           AND password_reset_token_hash IS NOT NULL
           AND password_reset_used_at IS NULL
           AND password_reset_expires_at > NOW()",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AuthError::InvalidResetToken)?;

    let stored_hash = user.password_reset_token_hash.as_deref().unwrap_or_default();
    let submitted_hash = sha256_hex(&req.token);
    use subtle::ConstantTimeEq;
    if stored_hash.as_bytes().ct_eq(submitted_hash.as_bytes()).unwrap_u8() != 1 {
        return Err(AuthError::InvalidResetToken);
    }

    let new_hash = password::hash_password(&req.new_password)?;
    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "UPDATE users
         SET password_hash = $1, password_reset_used_at = NOW()
         WHERE user_id = $2",
    )
    .bind(&new_hash)
    .bind(user.user_id)
    .execute(&mut *tx)
    .await?;

    // schema.rs's own comment on refresh_tokens.revoked_at: "set on
    // logout, password reset, or detected compromise" — a password
    // reset revokes every existing session, not just this one.
    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user.user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(StatusCode::OK)
}
