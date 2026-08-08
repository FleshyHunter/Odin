// Redis-staged Onboarding Diagnostic (deferred.md #4, PRD.md's
// Onboarding Diagnostic Steps 1-4). A draft DAG can't be safely
// persisted to Postgres before grading might still adjust it (a
// contradiction can reassign the entry concept, or extend the draft
// entirely via adjust_dag()) — so everything between `/journeys/start`
// and the student actually answering lives here first, same
// staged-then-committed shape as memoryless/staging.rs, only committed
// at finalize() rather than a separate "convert" step. key:
// onboarding_diagnostic:{diagnostic_id}, TTL from
// ONBOARDING_DIAGNOSTIC_TTL_MINUTES.

use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::errors::JourneyError;
use super::instantiate::InstantiatedExercise;
use crate::ai_client::{DAGResult, IntakeContext};
use crate::state::AppState;

fn diagnostic_key(diagnostic_id: Uuid) -> String {
    format!("onboarding_diagnostic:{diagnostic_id}")
}

/// Which subject this diagnostic is scoped against — a brand new one
/// (nothing in Postgres yet, the whole draft DAG travels with the
/// staged blob) or one that already exists canonically (only the id is
/// needed; its DAG is never mutated by this flow).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubjectContext {
    New {
        normalized_name: String,
        subject_title: String,
        draft: DAGResult,
    },
    Existing {
        subject_id: Uuid,
    },
}

/// A New subject has no concept_ids yet (canonical_concepts rows don't
/// exist until finalize() persists them) — the entry concept can only be
/// named by its title within SubjectContext::New's own draft until then.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryConcept {
    Draft(String),
    Existing(Uuid),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedDiagnostic {
    pub diagnostic_id: Uuid,
    pub user_id: Uuid,
    pub topic: String,
    pub intake: IntakeContext,
    pub subject: SubjectContext,
    pub entry_concept: EntryConcept,
    pub exercise: InstantiatedExercise,
    // Only ever Some() for a NEW subject — generate_dag()'s own doc:
    // diagnostic_backup is populated only alongside diagnostic_primary,
    // which only happens when intake_context was provided (the
    // new-subject branch). Reusing an existing subject's canonical
    // exercise never produces a backup.
    pub backup_exercise: Option<InstantiatedExercise>,
    // Set true the moment `respond()` finds a contradiction, so a
    // second /respond call (rather than confirm-downgrade/retry-backup)
    // against the same diagnostic_id is rejected instead of silently
    // grading twice.
    pub pending_confirmation: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn save(state: &AppState, staged: &StagedDiagnostic) -> Result<(), JourneyError> {
    let mut conn = state.get_redis_connection().await?;
    let json = serde_json::to_string(staged)?;
    let ttl_seconds = (state.onboarding_diagnostic_ttl_minutes * 60).max(1) as u64;
    conn.set_ex::<_, _, ()>(diagnostic_key(staged.diagnostic_id), json, ttl_seconds).await?;
    Ok(())
}

pub async fn load(state: &AppState, diagnostic_id: Uuid) -> Result<Option<StagedDiagnostic>, JourneyError> {
    let mut conn = state.get_redis_connection().await?;
    let raw: Option<String> = conn.get(diagnostic_key(diagnostic_id)).await?;
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// Same ownership-check shape as memoryless::staging::load_owned (Rule
/// 34): expired/never-existed and owned-by-someone-else are
/// deliberately distinguished (410 vs 404), for the same reason that
/// module already documents.
pub async fn load_owned(
    state: &AppState,
    diagnostic_id: Uuid,
    user_id: Uuid,
) -> Result<StagedDiagnostic, JourneyError> {
    let staged = load(state, diagnostic_id)
        .await?
        .ok_or(JourneyError::DiagnosticExpiredOrNotFound)?;
    if staged.user_id != user_id {
        return Err(JourneyError::NotFound);
    }
    Ok(staged)
}

/// Called only AFTER finalize()'s Postgres commit succeeds — same
/// commit-then-delete ordering as memoryless::handlers::convert, so a
/// failed finalize never loses the staged diagnostic.
pub async fn delete(state: &AppState, diagnostic_id: Uuid) -> Result<(), JourneyError> {
    let mut conn = state.get_redis_connection().await?;
    conn.del::<_, ()>(diagnostic_key(diagnostic_id)).await?;
    Ok(())
}
