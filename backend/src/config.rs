use std::env;

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
    // frontend makes real cross-origin requests (5173 -> 8080) with
    // credentials (the httpOnly refresh_token cookie). Deliberately one
    // explicit origin, not a wildcard: allow_credentials(true) can't be
    // combined with Any in tower-http anyway, and a real allow-list is
    // the point, not an accident of the API.
    pub frontend_origin: String,
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
    pub memoryless_thread_ttl_minutes: i64,
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
        let frontend_origin =
            env::var("FRONTEND_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".to_string());
        let ai_service_url = env::var("AI_SERVICE_URL").expect("AI_SERVICE_URL must be set");

        let memoryless_thread_ttl_minutes = env::var("MEMORYLESS_THREAD_TTL_MINUTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

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
            ai_service_url,
            memoryless_thread_ttl_minutes,
        }
    }
}
