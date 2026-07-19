// Access+refresh token pair (Auth section: "CHANGED — access+refresh
// pair, not a single 30-day JWT"). Access tokens are short-lived
// (~15 min) and used for actual requests; refresh tokens are long-lived
// (30 days), httpOnly-cookie-delivered (see handlers.rs), and silently
// mint new access tokens so a user effectively never sees a conscious
// "log in again" moment during normal use.

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub token_type: TokenType,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

fn issue(user_id: Uuid, token_type: TokenType, ttl: Duration, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        token_type,
        iat: now.timestamp(),
        exp: (now + ttl).timestamp(),
    };
    encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret.as_bytes()))
}

pub fn issue_access_token(user_id: Uuid, minutes: i64, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    issue(user_id, TokenType::Access, Duration::minutes(minutes), secret)
}

pub fn issue_refresh_token(user_id: Uuid, days: i64, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    issue(user_id, TokenType::Refresh, Duration::days(days), secret)
}

/// Decodes and verifies signature + expiry, then additionally checks
/// the token was issued as the expected type — an access token must
/// never be accepted where a refresh token is required, or vice versa,
/// even though both are structurally valid, signed JWTs.
pub fn verify(token: &str, expected_type: TokenType, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    if data.claims.token_type != expected_type {
        return Err(jsonwebtoken::errors::ErrorKind::InvalidToken.into());
    }
    Ok(data.claims)
}
