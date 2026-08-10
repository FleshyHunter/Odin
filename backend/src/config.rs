use std::env;

use crate::auth::rate_limit::parse_rate_limit;

// Block 1 only needed DATABASE_URL/DB_POOL_SIZE/PORT. Block 3 (auth)
// adds Redis + JWT + OTP + password config — all per the locked
// Environment Variables list, not invented here.
pub struct Config {
    pub database_url: String,
    pub db_pool_size: u32,
    pub port: u16,
    pub redis_url: String,
    pub jwt_secret: String,
    pub access_token_expiry_minutes: i64,
    pub refresh_token_expiry_days: i64,
    pub otp_expiry_minutes: i64,
    pub verified_signup_token_ttl_minutes: i64,
    pub password_min_length: usize,
    // Real email delivery (auth/email.rs's SmtpEmailSender) — a
    // deliberate swap from the originally-locked "Resend" provider
    // (ARCHITECTURE.md) to plain SMTP through a real Gmail account,
    // made explicitly when setting this up for real. Generic SMTP
    // config, not Gmail-specific, so a future swap to another provider
    // is just different env var values, not a code change. All three
    // optional together — auth still works via the console-log stub
    // when unset, per the Auth section's own "ordering note" (auth
    // isn't tightly sequenced against external service availability).
    pub smtp_relay: Option<String>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    // The browser's own same-origin policy, not an auth/authorization
    // layer (JWT + RLS already cover that) — needed the moment the
    // frontend makes real cross-origin requests (5174 -> 8080) with
    // credentials (the httpOnly refresh_token cookie). Deliberately one
    // explicit origin, not a wildcard: allow_credentials(true) can't be
    // combined with Any in tower-http anyway, and a real allow-list is
    // the point, not an accident of the API.
    pub frontend_origin: String,
    // Refresh cookie's Secure attribute (RULES.md Rule 49: "Secure in
    // production"). Defaults false so local HTTP dev keeps working — a
    // Secure cookie is silently refused by the browser over plain HTTP,
    // which would break every auth flow, not just look insecure. Set
    // COOKIE_SECURE=true once this is ever actually served over HTTPS
    // (deferred.md #16 — a one env-var flip now, not a source edit
    // someone has to remember at deploy time).
    pub cookie_secure: bool,
    // Block 5: FastAPI AI service, on the Windows RTX PC (locked env
    // var name and value shape, ARCHITECTURE.md's Environment
    // Variables list — e.g. http://100.125.58.90:8000).
    pub ai_service_url: String,
    // Block 11: Memoryless Mode's Redis-staged thread TTL (PRD.md,
    // Memoryless Mode). Sliding — reset on every new message, not a
    // fixed expiry from creation. Deliberately its own env var, not
    // shared with MEMORYLESS_STAGED_UPLOAD_TTL_MINUTES (that one's
    // Block-12+ territory and can legitimately differ — a large staged
    // upload arguably deserves a shorter, more deliberate window than
    // casual message history).
    //
    // NOTE (deferred.md #32): a genuinely separate upload TTL isn't
    // actually implemented — staged uploads live inside this SAME
    // thread blob/key, so they share this exact TTL today rather than
    // their own shorter one. Documented deviation, not a silent gap —
    // see deferred.md #32's own note for why (would need staged
    // uploads to move to their own Redis keys, a real but separate
    // piece of work from the size caps this pass actually builds).
    pub memoryless_thread_ttl_minutes: i64,
    // PRD.md, Memoryless Mode — SIZE GUARDRAIL: explicit limits on a
    // staged upload, enforced BEFORE embedding starts, so an oversized
    // real upload can't run uncapped OCR/chunk/embed compute (deferred.md
    // #32 — previously unenforced despite the doc already framing this
    // as "closes a real gap").
    pub memoryless_staged_upload_max_mb: u64,
    pub memoryless_staged_upload_max_chunks: usize,
    // Rule 12 (never hardcode thresholds) — deferred.md #29. Same
    // value PRD.md already locks (120s), now a real env var instead of
    // a compiled-in Rust constant.
    pub template_gen_lock_ttl_seconds: u64,
    // deferred.md #19: shared with ai_service's own RETRIEVAL_MIN_SCORE
    // (knowledge/service.py) — same locked concept, same threshold, per
    // PRD.md's "results from both are merged and ranked together"
    // (ChromaDB + this session's own staged-upload chunks). Genuinely
    // cross-language, not a duplicate invention.
    pub retrieval_min_score: f32,
    // Redis Phase 1 (deferred.md #9-11, PRD.md NC7's locked values) —
    // (max_count, window_seconds) pairs, parsed from the locked "N:Xm"/
    // "N:Xh" env shape.
    pub login_rate_limit: (u32, u64),
    pub otp_resend_rate_limit: (u32, u64),
    pub signup_rate_limit: (u32, u64),
    pub password_reset_rate_limit: (u32, u64),
    // deferred.md #78: no PRD-locked values exist for these three (all
    // postdate PRD.md's own NC7 rate-limit list) — defaults picked as
    // generous-but-real: journey creation is a heavier, rarer action
    // (10/hour); a real chat message rate limit needs to comfortably
    // clear fast human typing while still blocking a tight automated
    // loop (30/minute).
    pub journey_start_rate_limit: (u32, u64),
    pub journey_message_rate_limit: (u32, u64),
    pub memoryless_message_rate_limit: (u32, u64),
    // A bare count, not a window — "5 OTP verify attempts before
    // forcing a fresh code," not "5 per some time period."
    pub otp_verify_attempt_limit: u32,
    // deferred.md #4: how long a Redis-staged Onboarding Diagnostic
    // (journeys::staging) survives between `/journeys/start` and the
    // student actually answering — same "not yet durable, Redis-only
    // until finalized" shape as memoryless staging, but its own env var
    // since these are unrelated flows with no reason to share a value.
    pub onboarding_diagnostic_ttl_minutes: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        // Locked as "pool size 5-10" (Technology Stack) — 10 is the
        // upper bound already used as the example value in the
        // Environment Variables list, so it's the sensible default.
        let db_pool_size = env::var("DB_POOL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        // Not part of the locked env var list (that list covers the AI
        // service's Tailscale/Ollama ports, not the Rust server's own
        // listen port) — PORT is this server's own HTTP port, separate
        // from FastAPI's 8000 and Ollama's 11434 on the Windows box.
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");
        let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");

        let access_token_expiry_minutes = env::var("ACCESS_TOKEN_EXPIRY_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15);
        let refresh_token_expiry_days = env::var("REFRESH_TOKEN_EXPIRY_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        let otp_expiry_minutes = env::var("OTP_EXPIRY_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let verified_signup_token_ttl_minutes = env::var("VERIFIED_SIGNUP_TOKEN_TTL_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        let password_min_length = env::var("PASSWORD_MIN_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12);
        let smtp_relay = env::var("SMTP_RELAY").ok();
        let smtp_username = env::var("SMTP_USERNAME").ok();
        // Google's own UI displays an App Password grouped in 4s with
        // spaces ("abcd efgh ijkl mnop") purely for readability — the
        // real credential has none. Stripped defensively here so a
        // direct copy-paste from that screen (the natural thing to do)
        // doesn't silently fail SMTP auth; found live when this was
        // first set up (a space in an UNQUOTED .env value also broke
        // dotenvy's own parsing, a second, separate trap the quoting
        // fix in .env.example addresses, but this handles the value
        // itself regardless of quoting).
        let smtp_password = env::var("SMTP_PASSWORD").ok().map(|p| p.replace(' ', ""));
        // 5174, not Vite's own factory default of 5173 — a separate
        // local project ("veritas") already owns 5173 on this machine,
        // and frontend/vite.config.ts is pinned to 5174 for the same
        // reason. Silently falling back to a port the frontend isn't
        // actually running on breaks CORS for every credentialed
        // request with no obvious error — this default must stay in
        // sync with vite.config.ts's server.port.
        let frontend_origin =
            env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "http://localhost:5174".to_string());
        let cookie_secure = env::var("COOKIE_SECURE")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(false);
        let ai_service_url = env::var("AI_SERVICE_URL").expect("AI_SERVICE_URL must be set");

        // deferred.md #55: lowered from 60 (2026-08-06) — a deliberate
        // value decision, not a bug fix; the sliding anchor point and
        // expiry-race handling were both already correct before this.
        let memoryless_thread_ttl_minutes = env::var("MEMORYLESS_THREAD_TTL_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15);
        let memoryless_staged_upload_max_mb = env::var("MEMORYLESS_STAGED_UPLOAD_MAX_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(25);
        let memoryless_staged_upload_max_chunks = env::var("MEMORYLESS_STAGED_UPLOAD_MAX_CHUNKS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);
        let template_gen_lock_ttl_seconds = env::var("TEMPLATE_GEN_LOCK_TTL_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120);
        let retrieval_min_score = env::var("RETRIEVAL_MIN_SCORE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.60);

        // Defaults match PRD.md NC7's locked values exactly, so an
        // unset/fresh environment still gets real brute-force
        // protection rather than silently having none.
        let login_rate_limit = env::var("LOGIN_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((5, 15 * 60));
        let otp_resend_rate_limit = env::var("OTP_RESEND_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((3, 10 * 60));
        let signup_rate_limit = env::var("SIGNUP_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((3, 3600));
        let password_reset_rate_limit = env::var("PASSWORD_RESET_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((3, 3600));
        let otp_verify_attempt_limit = env::var("OTP_VERIFY_ATTEMPT_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        let onboarding_diagnostic_ttl_minutes = env::var("ONBOARDING_DIAGNOSTIC_TTL_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        let journey_start_rate_limit = env::var("JOURNEY_START_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((10, 3600));
        let journey_message_rate_limit = env::var("JOURNEY_MESSAGE_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((30, 60));
        let memoryless_message_rate_limit = env::var("MEMORYLESS_MESSAGE_RATE_LIMIT")
            .ok()
            .map(|v| parse_rate_limit(&v))
            .unwrap_or((30, 60));

        Self {
            database_url,
            db_pool_size,
            port,
            redis_url,
            jwt_secret,
            access_token_expiry_minutes,
            refresh_token_expiry_days,
            otp_expiry_minutes,
            verified_signup_token_ttl_minutes,
            password_min_length,
            smtp_relay,
            smtp_username,
            smtp_password,
            frontend_origin,
            cookie_secure,
            ai_service_url,
            memoryless_thread_ttl_minutes,
            memoryless_staged_upload_max_mb,
            memoryless_staged_upload_max_chunks,
            template_gen_lock_ttl_seconds,
            retrieval_min_score,
            login_rate_limit,
            otp_resend_rate_limit,
            signup_rate_limit,
            password_reset_rate_limit,
            otp_verify_attempt_limit,
            onboarding_diagnostic_ttl_minutes,
            journey_start_rate_limit,
            journey_message_rate_limit,
            memoryless_message_rate_limit,
        }
    }
}
