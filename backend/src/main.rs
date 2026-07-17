mod config;
mod db;
mod routes;

use config::Config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Loads backend/.env in dev; in real deployment these come from the
    // actual environment, not a file — ok() so a missing file (e.g. in
    // production) isn't a hard error.
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    let pool = db::connect(&config.database_url, config.db_pool_size)
        .expect("failed to create Postgres pool");

    let app = routes::router(pool);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");

    tracing::info!("odin-backend listening on port {}", config.port);

    axum::serve(listener, app).await.expect("server error");
}
