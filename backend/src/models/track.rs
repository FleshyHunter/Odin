// Models: Project, StudyThread, Message
// Tables: projects, study_threads, messages (migrations/0001_initial_schema.sql)
// "Track" is the frontend/UI-facing name for study_threads (SCHEMA.md's
// own comment: "Track = study_threads, one row, optionally with one
// journey attached") — this file groups the three tables that back
// that UI concept: the thread itself, its optional Project grouping,
// and its messages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Project {
    pub project_id: Uuid,
    pub user_id: Option<Uuid>,
    pub name: String,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct StudyThread {
    pub thread_id: Uuid,
    pub user_id: Option<Uuid>,
    pub journey_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub title: Option<String>,
    pub is_pinned: Option<bool>,
    pub mode: Option<String>,
    pub current_concept_id: Option<Uuid>,
    pub pre_tangent_concept_id: Option<Uuid>,
    pub state: Option<Json>,
    pub created_at: Option<DateTime<Utc>>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub thread_id: Option<Uuid>,
    pub role: Option<String>,
    pub content: String,
    pub mode: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}
