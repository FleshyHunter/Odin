// OTP mechanism (ARCHITECTURE.md, Auth section: OTP Mechanism). Shared
// by signup verification, standing OTP login, AND password reset — same
// code path, three different Redis key namespaces so none of them ever
// collide even for the same email.
//
//   signup_otp:{email}            TTL OTP_EXPIRY_MINUTES (10) — signup code
//   login_otp:{email}             TTL OTP_EXPIRY_MINUTES (10) — login code
//   password_reset_otp:{email}    TTL OTP_EXPIRY_MINUTES (10) — reset code
//   verified_signup:{email}          TTL VERIFIED_SIGNUP_TOKEN_TTL_MINUTES
//   verified_password_reset:{email}  (30) — set once the matching *_otp key
//                            verifies; checked (by email alone, no separate
//                            token needs round-tripping to the client) at
//                            the final submission step of whichever flow
//                            staged it. Password reset reuses the same TTL
//                            config value as signup — same "just-verified,
//                            about to submit" semantics either way, not
//                            worth a second near-identical env var for.
//   {signup,login,password_reset}_otp:{email}:attempts  — wrong-guess
//                            counter (deferred.md #9-11, OTP_VERIFY_ATTEMPT_
//                            LIMIT), same TTL as its sibling *_otp key.
//
// Hashing the OTP itself is a fast hash (SHA-256), deliberately NOT
// argon2 — the OTP's defense is time (short TTL) and attempt-count
// (rate limiting), not hash slowness. Comparison is constant-time to
// avoid leaking info about a partially-correct guess via timing.

use rand::RngExt;
use redis::{aio::MultiplexedConnection, AsyncCommands, RedisResult};
use subtle::ConstantTimeEq;

use super::hash_utils::sha256_hex;

fn signup_otp_key(email: &str) -> String {
    format!("signup_otp:{email}")
}

fn login_otp_key(email: &str) -> String {
    format!("login_otp:{email}")
}

fn verified_signup_key(email: &str) -> String {
    format!("verified_signup:{email}")
}

fn password_reset_otp_key(email: &str) -> String {
    format!("password_reset_otp:{email}")
}

fn verified_password_reset_key(email: &str) -> String {
    format!("verified_password_reset:{email}")
}

fn generate_code() -> String {
    let code: u32 = rand::rng().random_range(0..1_000_000);
    format!("{code:06}")
}

fn hash_code(code: &str) -> String {
    sha256_hex(code)
}

fn constant_time_matches(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Generates a fresh code, stages its hash in Redis (overwriting any
/// prior pending code for this email — a resend is just a fresh SET,
/// no separate invalidation logic needed), and returns the PLAINTEXT
/// code for the caller to email. Used for both `signup_otp` and
/// `login_otp` via the `key` closure parameter, so both share one
/// implementation rather than near-duplicating it per namespace.
async fn stage_otp(
    conn: &mut MultiplexedConnection,
    key: &str,
    ttl_minutes: i64,
) -> RedisResult<String> {
    let code = generate_code();
    let hash = hash_code(&code);
    let ttl_seconds = (ttl_minutes * 60).max(1) as u64;
    conn.set_ex::<_, _, ()>(key, hash, ttl_seconds).await?;
    Ok(code)
}

pub async fn stage_signup_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    ttl_minutes: i64,
) -> RedisResult<String> {
    stage_otp(conn, &signup_otp_key(email), ttl_minutes).await
}

pub async fn stage_login_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    ttl_minutes: i64,
) -> RedisResult<String> {
    stage_otp(conn, &login_otp_key(email), ttl_minutes).await
}

pub async fn stage_password_reset_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    ttl_minutes: i64,
) -> RedisResult<String> {
    stage_otp(conn, &password_reset_otp_key(email), ttl_minutes).await
}

/// Verifies a submitted code against the staged hash and deletes it on
/// success (single-use — a matched code can never be replayed).
/// Mismatch/expiry both just return false; the caller doesn't get to
/// distinguish "wrong code" from "expired," which is the point (no
/// extra information leaked to a guesser).
///
/// OTP_VERIFY_ATTEMPT_LIMIT (deferred.md #9-11, PRD.md NC7 — "5 OTP
/// verify attempts before forcing a fresh code"): a wrong guess
/// increments a sibling `{key}:attempts` counter, given the SAME TTL as
/// the OTP itself the first time it's set (so it can never outlive the
/// code it's counting attempts against). Hitting the limit deletes the
/// OTP outright — forcing a fresh `request-otp` call — rather than just
/// refusing further guesses, so there's no way to keep the same code
/// alive indefinitely by spacing out guesses.
async fn verify_otp(
    conn: &mut MultiplexedConnection,
    key: &str,
    submitted: &str,
    max_attempts: u32,
) -> RedisResult<bool> {
    let stored: Option<String> = conn.get(key).await?;
    let Some(stored_hash) = stored else {
        return Ok(false);
    };
    let matches = constant_time_matches(&stored_hash, &hash_code(submitted));
    let attempts_key = format!("{key}:attempts");
    if matches {
        conn.del::<_, ()>((key, attempts_key.as_str())).await?;
    } else {
        let attempts: u32 = conn.incr(&attempts_key, 1).await?;
        if attempts == 1 {
            // Align the attempts counter's lifetime with the OTP's own
            // remaining TTL, not a fresh one of its own.
            let ttl: i64 = conn.ttl(key).await?;
            if ttl > 0 {
                conn.expire::<_, ()>(&attempts_key, ttl).await?;
            }
        }
        if attempts >= max_attempts {
            conn.del::<_, ()>((key, attempts_key.as_str())).await?;
        }
    }
    Ok(matches)
}

pub async fn verify_signup_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    submitted: &str,
    max_attempts: u32,
) -> RedisResult<bool> {
    verify_otp(conn, &signup_otp_key(email), submitted, max_attempts).await
}

pub async fn verify_login_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    submitted: &str,
    max_attempts: u32,
) -> RedisResult<bool> {
    verify_otp(conn, &login_otp_key(email), submitted, max_attempts).await
}

pub async fn verify_password_reset_otp(
    conn: &mut MultiplexedConnection,
    email: &str,
    submitted: &str,
    max_attempts: u32,
) -> RedisResult<bool> {
    verify_otp(conn, &password_reset_otp_key(email), submitted, max_attempts).await
}

/// Marks this email as having just passed OTP verification, so the
/// final submission step of whichever flow staged it can confirm that
/// happened recently rather than trusting a stale, long-abandoned tab.
/// Shared by signup and password reset via the `key` closure parameter,
/// same reasoning as `stage_otp`/`verify_otp` above.
async fn mark_verified(conn: &mut MultiplexedConnection, key: &str, ttl_minutes: i64) -> RedisResult<()> {
    let ttl_seconds = (ttl_minutes * 60).max(1) as u64;
    conn.set_ex::<_, _, ()>(key, "1", ttl_seconds).await
}

/// Checks and consumes a verified marker (single-use, same reasoning as
/// the OTP codes themselves — a completed flow should not let a second,
/// replayed submission through).
async fn consume_verified(conn: &mut MultiplexedConnection, key: &str) -> RedisResult<bool> {
    let exists: bool = conn.exists(key).await?;
    if exists {
        conn.del::<_, ()>(key).await?;
    }
    Ok(exists)
}

pub async fn mark_signup_verified(
    conn: &mut MultiplexedConnection,
    email: &str,
    ttl_minutes: i64,
) -> RedisResult<()> {
    mark_verified(conn, &verified_signup_key(email), ttl_minutes).await
}

pub async fn consume_signup_verified(conn: &mut MultiplexedConnection, email: &str) -> RedisResult<bool> {
    consume_verified(conn, &verified_signup_key(email)).await
}

pub async fn mark_password_reset_verified(
    conn: &mut MultiplexedConnection,
    email: &str,
    ttl_minutes: i64,
) -> RedisResult<()> {
    mark_verified(conn, &verified_password_reset_key(email), ttl_minutes).await
}

pub async fn consume_password_reset_verified(conn: &mut MultiplexedConnection, email: &str) -> RedisResult<bool> {
    consume_verified(conn, &verified_password_reset_key(email)).await
}
