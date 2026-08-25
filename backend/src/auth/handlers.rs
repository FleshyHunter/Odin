use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    Json,
};
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
use super::rate_limit;
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

// Secure driven by config (COOKIE_SECURE, defaults false) rather than a
// hardcoded value — defaults off so local HTTP dev keeps working (a
// Secure cookie is silently refused by the browser over plain HTTP,
// which would break every auth flow, not just look insecure), but
// deploying over real HTTPS is now a one env-var flip, not a
// source-edit someone has to remember (deferred.md #16).
fn refresh_cookie(token: &str, days: i64, secure: bool) -> Cookie<'static> {
    Cookie::build(("refresh_token", token.to_string()))
        .http_only(true)
        .secure(secure)
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

    let jar = CookieJar::new().add(refresh_cookie(
        &issued.refresh_token,
        state.refresh_token_expiry_days,
        state.cookie_secure,
    ));
    let response = AuthResponse { access_token: issued.access_token, user: user.into() };
    Ok((jar, Json(response)))
}

// ---------------- Signup (OTP-first, 3 steps) ----------------

#[derive(Deserialize)]
pub struct EmailRequest {
    email: String,
}

// deferred.md #9-11 — no anti-enumeration concern on this endpoint (it
// never branches on whether the account exists, unlike login/password-
// reset's request-otp below), so both limits can 429 openly.
pub async fn signup_request_otp(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<StatusCode, AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;

    let (signup_max, signup_window) = state.signup_rate_limit;
    let signup_key = rate_limit::signup_key(&addr.ip());
    if !rate_limit::check_and_increment(&mut conn, &signup_key, signup_max, signup_window).await? {
        return Err(AuthError::RateLimited);
    }
    let (resend_max, resend_window) = state.otp_resend_rate_limit;
    let resend_key = rate_limit::otp_resend_key(&email);
    if !rate_limit::check_and_increment(&mut conn, &resend_key, resend_max, resend_window).await? {
        return Err(AuthError::RateLimited);
    }

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
    let matched = otp::verify_signup_otp(&mut conn, &email, &req.code, state.otp_verify_attempt_limit).await?;
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

    // Found live 2026-08-25: validate BEFORE consuming the proof token,
    // not after. consume_signup_verified deletes the token unconditionally
    // the moment it's called — with validation running afterward, a
    // rejected password (too short, etc.) still burned the one-time
    // token on that first attempt, so every retry with a CORRECTED
    // password then hit "verification expired" even though the real OTP
    // proof was still well within its 30-minute TTL. The token should
    // only ever be spent on an attempt that's actually going to succeed
    // past this point, not on every attempt regardless of outcome.
    password::validate_password(&req.password, state.password_min_length)
        .map_err(AuthError::Validation)?;

    let mut conn = state.get_redis().await?;

    // Step 3 only proceeds if step 2 actually succeeded recently for
    // THIS email (ARCHITECTURE.md: "so a stale, long-abandoned tab
    // can't submit credentials off an OTP match from much earlier").
    if !otp::consume_signup_verified(&mut conn, &email).await? {
        return Err(AuthError::VerificationRequired);
    }

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

    // deferred.md #9-11 — checked BEFORE ever touching Postgres, and
    // keyed regardless of whether the account is real (no enumeration
    // risk: this endpoint already returns the same InvalidCredentials
    // either way, so a 429 here doesn't reveal anything new).
    let mut conn = state.get_redis().await?;
    let (max, window) = state.login_rate_limit;
    let key = rate_limit::login_key(&email);
    if !rate_limit::check_and_increment(&mut conn, &key, max, window).await? {
        return Err(AuthError::RateLimited);
    }

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
    let mut conn = state.get_redis().await?;

    // Same login-attempt limit as login_password (ARCHITECTURE.md:
    // "login attempts (both methods)" is one shared limit) — safe to
    // 429 openly, same reasoning as login_password's own check.
    let (login_max, login_window) = state.login_rate_limit;
    let login_key = rate_limit::login_key(&email);
    if !rate_limit::check_and_increment(&mut conn, &login_key, login_max, login_window).await? {
        return Err(AuthError::RateLimited);
    }

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_normalized = $1)")
        .bind(&email)
        .fetch_one(&state.pool)
        .await?;

    // Always 200 regardless of whether the account exists — avoids
    // letting this endpoint be used to enumerate registered emails.
    // Only actually stages/sends a code when there's a real account.
    if exists {
        // deferred.md #9-11: the resend limit is checked HERE, inside
        // the exists branch, so it can only ever be reached for a real
        // account — a 429 for it would leak exactly that. Silently
        // skipping the send (still 200 either way) preserves the same
        // anti-enumeration guarantee as the `exists` check itself.
        let (resend_max, resend_window) = state.otp_resend_rate_limit;
        let resend_key = rate_limit::otp_resend_key(&email);
        if rate_limit::check_and_increment(&mut conn, &resend_key, resend_max, resend_window).await? {
            let code = otp::stage_login_otp(&mut conn, &email, state.otp_expiry_minutes).await?;
            state.email_sender.send_otp(&email, &code).await;
        }
    }
    Ok(StatusCode::OK)
}

pub async fn login_verify_otp(
    State(state): State<AppState>,
    Json(req): Json<VerifyOtpRequest>,
) -> Result<(CookieJar, Json<AuthResponse>), AuthError> {
    let email = normalize_email(&req.email);
    let mut conn = state.get_redis().await?;
    let matched = otp::verify_login_otp(&mut conn, &email, &req.code, state.otp_verify_attempt_limit).await?;
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

    let jar = CookieJar::new().add(refresh_cookie(
        &issued.refresh_token,
        state.refresh_token_expiry_days,
        state.cookie_secure,
    ));
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
    let mut conn = state.get_redis().await?;

    // deferred.md #9-11 — checked regardless of whether the account is
    // real (safe to 429 openly, same reasoning as login's own check).
    let (pr_max, pr_window) = state.password_reset_rate_limit;
    let pr_key = rate_limit::password_reset_key(&email);
    if !rate_limit::check_and_increment(&mut conn, &pr_key, pr_max, pr_window).await? {
        return Err(AuthError::RateLimited);
    }

    // Same anti-enumeration shape as login_request_otp — always 200,
    // only actually stages/sends a code when the account is real.
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_normalized = $1)")
        .bind(&email)
        .fetch_one(&state.pool)
        .await?;

    if exists {
        // Same silent-skip-on-limit shape as login_request_otp, for the
        // same enumeration reason — only reachable for a real account.
        let (resend_max, resend_window) = state.otp_resend_rate_limit;
        let resend_key = rate_limit::otp_resend_key(&email);
        if rate_limit::check_and_increment(&mut conn, &resend_key, resend_max, resend_window).await? {
            let code = otp::stage_password_reset_otp(&mut conn, &email, state.otp_expiry_minutes).await?;
            state.email_sender.send_otp(&email, &code).await;
        }
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
    let matched =
        otp::verify_password_reset_otp(&mut conn, &email, &req.code, state.otp_verify_attempt_limit).await?;
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

    // Same reordering fix as signup_complete (2026-08-25) — validate
    // BEFORE consuming the one-time proof token, so a rejected password
    // doesn't burn it and force starting the whole OTP flow over again
    // just to retry with a corrected password.
    password::validate_password(&req.new_password, state.password_min_length)
        .map_err(AuthError::Validation)?;

    let mut conn = state.get_redis().await?;

    // Same "step 3 only proceeds if step 2 actually succeeded recently
    // for THIS email" guard as signup_complete.
    if !otp::consume_password_reset_verified(&mut conn, &email).await? {
        return Err(AuthError::VerificationRequired);
    }

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
