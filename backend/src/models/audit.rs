// Models: AuditLog
// Tables: audit_logs (migrations/0001_initial_schema.sql)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct AuditLog {
    pub log_id: Uuid,
    pub thread_id: Option<Uuid>,
    pub user_input: Option<String>,
    pub cleaned_query: Option<String>,
    pub matched_concepts: Option<Json>,
    pub detected_intent: Option<String>,
    pub mode: Option<String>,
    pub retrieved_chunk_ids: Option<Json>,
    pub strategy_used: Option<String>,
    pub response_text: Option<String>,
    pub assessment_result: Option<Json>,
    pub model_used: Option<String>,
    pub prompt_version: Option<String>,
    pub error: Option<String>,
    pub queued_ms: Option<i32>,
    pub generation_ms: Option<i32>,
    pub timestamp: Option<DateTime<Utc>>,
}
