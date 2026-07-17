use std::env;

// Block 1 only needs enough config to boot the server and connect to
// Postgres — DATABASE_URL and DB_POOL_SIZE, per the locked
// Environment Variables list (Combined_Architecture_File.md). Later
// blocks (auth, Redis, Dify, etc.) add their own env vars to this
// struct as those blocks are built — not pulled in ahead of need.
pub struct Config {
    pub database_url: String,
    pub db_pool_size: u32,
    pub port: u16,
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

        Self { database_url, db_pool_size, port }
    }
}
