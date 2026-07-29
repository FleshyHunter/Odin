use axum::{extract::State, http::StatusCode, Json};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
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

// Rule 49 locks this to the narrow path (/auth/refresh) — logout lives
// at the SAME path via a different HTTP method (DELETE /auth/refresh,
// see router()) rather than widening this, since a browser matches
// cookies by path, not method. That keeps the cookie's exposure exactly
// as narrow as the rule requires while still letting logout read it.
const REFRESH_COOKIE_PATH: &str = "/auth/refresh";

fn refresh_cookie(token: &str, days: i64) -> Cookie<'static> {
    // Secure(true) omitted deliberately for now — this is plain HTTP in
    // local dev (see Rule 49's "Secure in production"); flip on once
    // this is ever actually served over HTTPS.
    Cookie::build(("refresh_token", token.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path(REFRESH_COOKIE_PATH)
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

// Rotation + sliding expiry (what actually makes normal use "never see
// a login screen," per Auth section — a single fixed-length refresh
// token, even at 30 days, would still eventually force a re-login
// regardless of activity). Every successful refresh: 1) revokes the
// refresh token that was just used (so a stolen/replayed copy of THIS
// exact cookie fails from this point on), 2) mints a brand-new
// access+refresh pair with a full fresh 30-day window, same as a fresh
// login. Normal regular use therefore keeps extending the window
// faster than it can expire; only real, sustained inactivity (not
// opening the app for a full window) ever forces a real sign-in again.
pub async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<RefreshResponse>), AuthError> {
    let raw_token = jar
        .get("refresh_token")
        .map(|c| c.value().to_string())
        .ok_or(AuthError::InvalidRefreshToken)?;

    let claims = jwt::verify(&raw_token, TokenType::Refresh, &state.jwt_secret)
        .map_err(|_| AuthError::InvalidRefreshToken)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AuthError::InvalidRefreshToken)?;

    let token_hash = sha256_hex(&raw_token);
    // refresh_tokens is RLS-protected — a plain pool query here would
    // silently see zero rows (no app.current_user_id set), rejecting
    // every refresh regardless of validity. We already have user_id
    // from the verified JWT, so scope the read to it.
    let mut tx = super::middleware::begin_rls_transaction(&state.pool, user_id).await?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM refresh_tokens
            WHERE user_id = $1 AND token_hash = $2
              AND revoked_at IS NULL AND expires_at > NOW()
        )",
    )
    .bind(user_id)
    .bind(&token_hash)
    .fetch_one(&mut *tx)
    .await?;

    if !valid {
        return Err(AuthError::InvalidRefreshToken);
    }

    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND token_hash = $2")
        .bind(user_id)
        .bind(&token_hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let issued = issue_tokens(
        &state.pool,
        user_id,
        &state.jwt_secret,
        state.access_token_expiry_minutes,
        state.refresh_token_expiry_days,
    )
    .await?;

    let jar = CookieJar::new().add(refresh_cookie(&issued.refresh_token, state.refresh_token_expiry_days));
    Ok((jar, Json(RefreshResponse { access_token: issued.access_token })))
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<(CookieJar, StatusCode), AuthError> {
    if let Some(raw_token) = jar.get("refresh_token").map(|c| c.value().to_string()) {
        // Best-effort revoke: an already-expired/garbage refresh token
        // has nothing meaningful left to revoke server-side, so logout
        // still succeeds and clears the cookie either way — only a
        // token that still parses gets its DB row revoked.
        if let Ok(claims) = jwt::verify(&raw_token, TokenType::Refresh, &state.jwt_secret)
            && let Ok(user_id) = Uuid::parse_str(&claims.sub)
        {
            let token_hash = sha256_hex(&raw_token);
            // Same RLS scoping issue as refresh/issue_tokens — this
            // UPDATE has to run inside a transaction with
            // app.current_user_id set, or it silently revokes zero
            // rows against the RLS-protected refresh_tokens table.
            let mut tx = super::middleware::begin_rls_transaction(&state.pool, user_id).await?;
            sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE token_hash = $1 AND revoked_at IS NULL")
                .bind(&token_hash)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }
    // A removal Set-Cookie only actually overwrites the original in a
    // real browser if its Path matches exactly — explicit here rather
    // than relying on this handler's own request path (which now is
    // /auth/refresh too, see router(), but that's an implementation
    // detail this shouldn't depend on).
    let cleared = jar.remove(Cookie::build("refresh_token").path(REFRESH_COOKIE_PATH).build());
    Ok((cleared, StatusCode::NO_CONTENT))
}

// ---------------- Password reset (OTP-based — same mechanism as
// signup/login, not the earlier link+token design; see Auth section's
// "Frontend Wiring"/password-reset note for why this changed) ----------------

pub async fn password_reset_request_otp(
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);

    // Same anti-enumeration shape as login_request_otp — always 200,
    // only actually stages/sends a code when the account is real.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_normalized = $1)")
        .bind(&email)
        .fetch_one(&state.pool)
        .await?;

    if exists {
        let mut conn = state.get_redis().await?;
        let code = otp::stage_password_reset_otp(&mut conn, &email, state.otp_expiry_minutes).await?;
        state.email_sender.send_otp(&email, &code).await;
    }

    Ok(StatusCode::OK)
}

#[derive(Serialize)]
pub struct PasswordResetVerifyResponse {
    display_name: String,
}

pub async fn password_reset_verify_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> Result<Json<PasswordResetVerifyResponse>, AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;
    let matched = otp::verify_password_reset_otp(&mut conn, &email, &req.code).await?;
    if !matched {
        return Err(AuthError::InvalidOtp);
    }
    otp::mark_password_reset_verified(&mut conn, &email, state.verified_signup_token_ttl_minutes).await?;

    // Safe to reveal here (unlike request_otp above): a matching code
    // could only ever have been staged for a real, existing account, so
    // this doesn't leak anything a successful OTP match hasn't already
    // proven.
    let display_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE email_normalized = $1")
        .bind(&email)
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(PasswordResetVerifyResponse { display_name }))
}

#[derive(Deserialize)]
pub struct PasswordResetCompleteRequest {
    email: String,
    new_password: String,
}

pub async fn password_reset_complete(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetCompleteRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;

    // Same "step 3 only proceeds if step 2 actually succeeded recently
    // for THIS email" guard as signup_complete.
    if !otp::consume_password_reset_verified(&mut conn, &email).await? {
        return Err(AuthError::VerificationRequired);
    }

    password::validate_password(&req.new_password, state.password_min_length)
        .map_err(AuthError::Validation)?;
    let new_hash = password::hash_password(&req.new_password)?;

    let user = sqlx::query_as::<_, User>(
        "UPDATE users SET password_hash = $1 WHERE email_normalized = $2 RETURNING *",
    )
    .bind(&new_hash)
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AuthError::InvalidCredentials)?;

    // users has no RLS (queryable by email pre-session, per
    // migrations/0002's own note), but refresh_tokens below does — this
    // transaction needs app.current_user_id set for that UPDATE to
    // actually revoke anything instead of silently affecting zero rows.
    let mut tx = super::middleware::begin_rls_transaction(&state.pool, user.user_id).await?;

    // schema.rs's own comment on refresh_tokens.revoked_at: "set on
    // logout, password reset, or detected compromise" — a password
    // reset revokes every existing session, not just this one.
    sqlx::query("UPDATE refresh_tokens SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user.user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // Auto-login: the caller already proved mailbox ownership via OTP
    // AND just set fresh credentials, so a third "now log in again"
    // step would be pure friction, not extra security.
    respond_with_tokens(&state, user).await
}
