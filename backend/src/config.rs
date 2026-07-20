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
    pub password_reset_token_expiry_hours: i64,
    // None until Resend is actually set up — auth still works, using
    // EmailSender's console-log stub instead of a real send (see
    // auth/email.rs). Not a blocker per the Auth section's own
    // "ordering note": auth details aren't tightly sequenced against
    // external service availability. Not read yet — will be, the
    // moment a ResendEmailSender impl exists to consume it.
    #[allow(dead_code)]
    pub resend_api_key: Option<String>,
    // Block 5: FastAPI AI service, on the Windows RTX PC (locked env
    // var name and value shape, ARCHITECTURE.md's Environment
    // Variables list — e.g. http://100.125.58.90:8000).
    pub ai_service_url: String,
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
        let password_reset_token_expiry_hours = env::var("PASSWORD_RESET_TOKEN_EXPIRY_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let resend_api_key = env::var("RESEND_API_KEY").ok();
        let ai_service_url = env::var("AI_SERVICE_URL").expect("AI_SERVICE_URL must be set");

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
            password_reset_token_expiry_hours,
            resend_api_key,
            ai_service_url,
        }
    }
}
