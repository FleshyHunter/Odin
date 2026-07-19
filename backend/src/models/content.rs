// Models: Source, Chunk, ContentFlag
// Tables: sources, chunks, content_flags (migrations/0001_initial_schema.sql)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Source {
    pub source_id: Uuid,
    pub url: Option<String>,
    pub filename: Option<String>,
    pub title: Option<String>,
    pub author_org: Option<String>,
    pub source_type: String,
    pub upload_role: String,
    pub upload_scope: Option<String>,
    pub journey_id: Option<Uuid>,
    pub license_status: Option<String>,
    pub trust_score: f32,
    pub content_hash: Option<String>,
    pub retrieval_date: Option<DateTime<Utc>>,
    pub subject_id: Option<Uuid>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Chunk {
    pub chunk_id: Uuid,
    pub source_id: Option<Uuid>,
    pub concept_id: Option<Uuid>,
    pub chunk_type: Option<String>,
    pub difficulty: Option<String>,
    pub text: String,
    pub token_count: Option<i32>,
    pub chroma_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ContentFlag {
    pub flag_id: Uuid,
    pub user_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub chunk_id: Option<Uuid>,
    pub exercise_id: Option<Uuid>,
    pub reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}
