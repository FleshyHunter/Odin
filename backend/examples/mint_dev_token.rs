// Scratch/throwaway tool for local verification ONLY (not part of the
// app) — mints a valid access-token JWT signed with the same
// JWT_SECRET the running server uses. Delete this file once
// verification is done.
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
struct Claims {
    sub: String,
    token_type: &'static str,
    exp: i64,
    iat: i64,
}

fn main() {
    dotenvy::dotenv().ok();
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let user_id = Uuid::new_v4();
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        token_type: "access",
        iat: now.timestamp(),
        exp: (now + Duration::minutes(30)).timestamp(),
    };
    let token = encode(&Header::new(Algorithm::HS256), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .expect("failed to encode token");
    println!("user_id={user_id}");
    println!("token={token}");
}
