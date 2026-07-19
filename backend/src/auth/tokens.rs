// "Both paths terminate at the same 'identity confirmed, issue tokens'
// step — built once, shared by both, not two separate implementations."
// (ARCHITECTURE.md, Auth section). Signup-completion and both login
// methods all call this one function.

use sqlx::PgPool;
use uuid::Uuid;

use super::errors::AuthError;
use super::hash_utils::sha256_hex;
use super::jwt;

pub struct IssuedTokens {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn issue_tokens(
    pool: &PgPool,
    user_id: Uuid,
    jwt_secret: &str,
    access_minutes: i64,
    refresh_days: i64,
) -> Result<IssuedTokens, AuthError> {
    let access_token = jwt::issue_access_token(user_id, access_minutes, jwt_secret)?;
    let refresh_token = jwt::issue_refresh_token(user_id, refresh_days, jwt_secret)?;

    // refresh_tokens stores a hash, never the raw token (Rule 49) — a
    // fast hash is fine here for the same reason it's fine for OTPs:
    // the token itself is long/high-entropy (a signed JWT), so its
    // defense is the token's own entropy + revoked_at/expires_at, not
    // hash slowness.
    let token_hash = sha256_hex(&refresh_token);
    let expires_at = chrono::Utc::now() + chrono::Duration::days(refresh_days);

    sqlx::query(
        "INSERT INTO refresh_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(IssuedTokens { access_token, refresh_token })
}
