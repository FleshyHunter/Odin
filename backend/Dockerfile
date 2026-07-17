# ---- Build stage ----
FROM rust:1-slim-bookworm AS builder
WORKDIR /app

# Cache dependency compilation separately from source changes: build a
# throwaway main.rs against just the manifests first, so `cargo build`
# only recompiles our own code (not every dependency) when src/ changes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
# COPY can leave main.rs with an older mtime than the cached dummy
# build's output, which would make cargo skip rebuilding it — touch
# forces a real rebuild against the actual source.
RUN touch src/main.rs && cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim
# ca-certificates: sqlx's tls-rustls feature verifies the Postgres
# connection against real root certs — needs this even though there's
# no other native TLS/OpenSSL dependency (tls-rustls is pure Rust).
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/odin-backend ./odin-backend

EXPOSE 8080
ENTRYPOINT ["./odin-backend"]
