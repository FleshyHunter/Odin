// argon2 hashing (Auth section: "not bcrypt — chosen as the more
// modern, currently recommended default for new systems") and the
// NIST-style password rule: minimum length only, no forced
// uppercase/number/special-character complexity (ARCHITECTURE.md,
// Password requirements — forced complexity pushes people toward
// predictable, rule-satisfying-but-weak patterns).

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// Minimum length only (min_length is the configured
/// PASSWORD_MIN_LENGTH, default 12) — deliberately no complexity
/// rules. "Optional: check against a known-breached-password list" is
/// noted in the doc but not built here — no breach-list dependency
/// exists yet in this project, and it's marked optional, not required.
pub fn validate_password(password: &str, min_length: usize) -> Result<(), String> {
    if password.chars().count() < min_length {
        return Err(format!("Password must be at least {min_length} characters"));
    }
    Ok(())
}
