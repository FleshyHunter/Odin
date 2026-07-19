// Models: Subject, CanonicalConcept, ConceptAlias, SubjectConcept, SubjectPrerequisite
// Tables: subjects, canonical_concepts, concept_aliases, subject_concepts,
//         subject_prerequisites (migrations/0001_initial_schema.sql)
// Canonical/shared knowledge — no RLS (SCHEMA.md: genuinely,
// unconditionally shared across accounts, unlike journey/track data).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Subject {
    pub subject_id: Uuid,
    pub title: String,
    pub normalized_name: String,
    pub description: Option<String>,
    pub dag_version: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct CanonicalConcept {
    pub concept_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct ConceptAlias {
    pub alias_id: Uuid,
    pub concept_id: Option<Uuid>,
    pub subject_id: Option<Uuid>,
    pub alias: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SubjectConcept {
    pub subject_concept_id: Uuid,
    pub subject_id: Option<Uuid>,
    pub concept_id: Option<Uuid>,
    pub learning_objective: Option<String>,
    pub difficulty_level: Option<i32>,
    pub order_index: Option<i32>,
    pub generated_by: Option<String>,
    pub confidence: Option<f32>,
    pub dag_version: Option<i32>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct SubjectPrerequisite {
    pub subject_id: Uuid,
    pub concept_id: Uuid,
    pub prereq_concept_id: Uuid,
    pub strength: Option<String>,
    pub dag_version: i32,
}
