// Models: User, RefreshToken
// Tables: users, refresh_tokens (migrations/0001_initial_schema.sql)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub email_normalized: String,
    pub password_hash: String,
    pub email_verified: bool,
    pub password_reset_token_hash: Option<String>,
    pub password_reset_expires_at: Option<DateTime<Utc>>,
    pub password_reset_used_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct RefreshToken {
    pub token_id: Uuid,
    pub user_id: Option<Uuid>,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}
