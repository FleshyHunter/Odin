// Models: Journey, JourneyConcept, JourneyPrerequisite, MasteryBank
// Tables: journeys, journey_concepts, journey_prerequisites, mastery_bank
// (migrations/0001_initial_schema.sql)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Journey {
    pub journey_id: Uuid,
    pub user_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub goal: Option<String>,
    pub initial_level_self_report: Option<String>,
    pub initial_level_diagnostic_result: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub last_active_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct JourneyConcept {
    pub journey_concept_id: Uuid,
    pub journey_id: Option<Uuid>,
    pub concept_id: Option<Uuid>,
    pub status: Option<String>,
    pub order_index: Option<i32>,
    pub is_custom_ordered: Option<bool>,
    pub advanced_correct_streak: Option<i32>,
    pub attempt_count: Option<i32>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub foundation_gap: Option<bool>,
    pub kiv_flagged_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct JourneyPrerequisite {
    pub journey_id: Uuid,
    pub concept_id: Uuid,
    pub prereq_concept_id: Uuid,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct MasteryBank {
    pub user_id: Uuid,
    pub concept_id: Uuid,
    pub mastery_score: Option<f32>,
    pub is_complete: Option<bool>,
    pub total_attempts: Option<i32>,
    pub last_assessed_at: Option<DateTime<Utc>>,
}
