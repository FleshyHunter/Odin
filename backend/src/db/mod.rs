use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

// Postgres itself runs on the Windows RTX PC (Hardware Distribution),
// reached over the Tailscale mesh — this pool is never talking to a
// local database; DATABASE_URL points at the Windows Tailscale IP.
//
// connect_lazy (not connect) deliberately: per the locked "no offline
// mode" section, Windows being unreachable means tutoring is down, but
// the server process itself shouldn't hard-crash at startup just
// because Postgres had a momentary blip — same behavior as any hosted
// app whose backend DB is briefly unreachable, which serves a
// degraded /health rather than refusing to boot at all. Actual
// connections are only established on first real use; routes/health.rs
// pings the pool per-request and reports "degraded" if that fails.
pub fn connect(database_url: &str, pool_size: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(pool_size)
        // Without this, a request against an unreachable Windows box
        // hangs on the OS's own TCP connect timeout (can be a minute-
        // plus) instead of failing fast. 5s matches "fail clearly and
        // quickly" rather than "no offline mode" meaning "hang forever."
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy(database_url)
}
