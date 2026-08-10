// Redis Phase 1 (ARCHITECTURE.md, Redis Phasing / PRD.md NC7 — deferred.md
// #9-11): brute-force/spam protection on the auth endpoints. Plain
// fixed-window counters (INCR + EXPIRE-on-first-increment) — matches
// Redis Phasing's own explicit rejection of anything fancier (Redis
// Streams etc.) "at this scale."

use std::net::IpAddr;

use redis::{aio::MultiplexedConnection, AsyncCommands, RedisResult};
use uuid::Uuid;

/// Parses the locked "N:Xm"/"N:Xh"/"N:Xs" env var shape (e.g. "5:15m")
/// into (max_count, window_seconds). Panics on a malformed value —
/// read once at config load (Rule: fail clearly, fail early), not
/// per-request, same philosophy as JWT_SECRET/OLLAMA_HOST's own env
/// parsing.
pub fn parse_rate_limit(value: &str) -> (u32, u64) {
    let (count_str, window_str) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("rate limit must be N:duration (e.g. 5:15m), got {value:?}"));
    let count: u32 = count_str
        .parse()
        .unwrap_or_else(|_| panic!("rate limit count must be a number, got {count_str:?}"));
    let (digits, unit) = window_str.split_at(window_str.len().saturating_sub(1));
    let n: u64 = digits
        .parse()
        .unwrap_or_else(|_| panic!("rate limit window must be a number followed by s/m/h, got {window_str:?}"));
    let seconds = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        _ => panic!("rate limit window unit must be s, m, or h, got {window_str:?}"),
    };
    (count, seconds)
}

/// Increments `key`, setting its expiry only on the first increment of
/// a fresh window (the standard Redis fixed-window pattern) — returns
/// true if this request is still within the limit, false if it just
/// tipped over. Callers own deciding what "over the limit" means for
/// their endpoint (a hard 429, or a silent no-op to preserve an
/// anti-enumeration guarantee — see handlers.rs's own comments).
pub async fn check_and_increment(
    conn: &mut MultiplexedConnection,
    key: &str,
    max_count: u32,
    window_seconds: u64,
) -> RedisResult<bool> {
    let count: u32 = conn.incr(key, 1).await?;
    if count == 1 {
        conn.expire::<_, ()>(key, window_seconds as i64).await?;
    }
    Ok(count <= max_count)
}

// One shared counter per email across ALL THREE OTP-issuing flows
// (signup/login/password-reset) — the abuse this guards against
// (spamming someone's inbox) is the same regardless of which flow
// triggered it, so a single combined counter is the simpler, more
// defensible reading of "3 OTP resends/10min" than three separate ones.
pub fn otp_resend_key(email: &str) -> String {
    format!("rate_limit:otp_resend:{email}")
}

// Shared across BOTH login methods (password and OTP) — ARCHITECTURE.md's
// own text: "login attempts (both methods)" is one limit, not two.
pub fn login_key(email: &str) -> String {
    format!("rate_limit:login:{email}")
}

// Per-IP, not per-email (PRD.md NC7: "3 signup attempts/hour/IP") —
// the one rate limit here that isn't email-keyed, since a signup
// attempt doesn't yet correspond to any account.
pub fn signup_key(ip: &IpAddr) -> String {
    format!("rate_limit:signup:{ip}")
}

pub fn password_reset_key(email: &str) -> String {
    format!("rate_limit:password_reset:{email}")
}

// deferred.md #78: the three real, GPU-triggering generation endpoints
// (#4/#18/#20) that were never given a rate limit once they stopped
// being bare, uncalled functions. User-keyed, not IP-keyed, like the
// email-keyed auth limits above — every caller here is already
// authenticated by the time these run, so the account itself (not the
// network address) is the meaningful identity to throttle.
pub fn journey_start_key(user_id: Uuid) -> String {
    format!("rate_limit:journey_start:{user_id}")
}

pub fn journey_message_key(user_id: Uuid) -> String {
    format!("rate_limit:journey_message:{user_id}")
}

pub fn memoryless_message_key(user_id: Uuid) -> String {
    format!("rate_limit:memoryless_message:{user_id}")
}
