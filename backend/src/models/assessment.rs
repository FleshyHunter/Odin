// Models: Exercise, QuizAttempt, KivReviewSession
// Tables: exercises, quiz_attempts, kiv_review_sessions
// (migrations/0001_initial_schema.sql)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Exercise {
    pub exercise_id: Uuid,
    pub concept_id: Option<Uuid>,
    pub exercise_type: Option<String>,
    pub difficulty: Option<String>,
    pub template_body: Json,
    pub template_params: Option<Json>,
    pub correct_answer: Option<String>,
    pub grader_type: Option<String>,
    pub grader_config: Option<Json>,
    pub tolerance: Option<f32>,
    pub is_canonical: Option<bool>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct QuizAttempt {
    pub attempt_id: Uuid,
    pub user_id: Option<Uuid>,
    pub exercise_id: Option<Uuid>,
    pub thread_id: Option<Uuid>,
    pub journey_id: Option<Uuid>,
    pub rendered_question: Option<String>,
    pub instantiated_params: Option<Json>,
    pub student_answer: Option<String>,
    pub expected_answer: Option<String>,
    pub is_correct: Option<bool>,
    pub score: Option<f32>,
    pub grader_used: Option<String>,
    pub feedback: Option<String>,
    pub difficulty_attempted: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct KivReviewSession {
    pub review_session_id: Uuid,
    pub user_id: Option<Uuid>,
    pub journey_id: Option<Uuid>,
    pub review_type: Option<String>,
    pub concept_ids: Option<Json>,
    pub triggered_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}
