// Shared fast-hash (SHA-256) helper — used for OTP codes, refresh
// tokens, and password reset tokens. All three share the same
// reasoning (ARCHITECTURE.md, OTP Mechanism): the thing being hashed is
// either short-lived (OTP, reset token) or high-entropy (a signed JWT
// refresh token), so time/entropy is the real defense, not argon2's
// deliberate slowness — using argon2 here would just cost CPU with no
// real security gain.

use sha2::{Digest, Sha256};

pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}
