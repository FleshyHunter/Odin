// Race prevention for generate_exercise_template() (markdown/
// deferred.md #3, #26). Two layers, different jobs (PRD.md, Exercise
// Template Generation — Race Prevention):
//   Correctness layer (already exists): a partial UNIQUE index on
//     exercises(concept_id, difficulty) WHERE is_canonical = TRUE
//     (migrations/0001_initial_schema.sql) — the actual guarantee, no
//     duplicate canonical template per difficulty can ever exist.
//   Cost layer (this file): a Redis SET NX lock, key
//     generating_template:{concept_id}, TTL 120s — an optimization on
//     top, not instead of. Only avoids a redundant, real Claude/Dify
//     call; the database constraint above is what actually prevents
//     duplicate data even if this lock's guarantee somehow fails.
//
// No real automatic caller exists yet (exercise-template
// prefetch-on-arrival needs journey/DAG creation, which doesn't exist
// — same gap as #4). Exposed as a real, standalone, directly-callable
// endpoint so it's independently testable now, same pattern already
// used for every ai_client:: function before its first real caller.

use std::time::Duration;

use redis::AsyncCommands;
use sqlx::PgPool;
use uuid::Uuid;

use crate::ai_client::{self, Chunk, ConceptMeta, ExerciseTemplate};
use crate::state::AppState;

use super::errors::ExerciseError;

const LOCK_TTL_SECONDS: u64 = 120; // PRD.md-locked value.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

fn lock_key(concept_id: Uuid) -> String {
    format!("generating_template:{concept_id}")
}

pub async fn get_or_generate(
    state: &AppState,
    concept_id: Uuid,
    concept_meta: ConceptMeta,
    top_chunks: Vec<Chunk>,
    batch_children: Vec<Uuid>,
) -> Result<Vec<ExerciseTemplate>, ExerciseError> {
    let key = lock_key(concept_id);
    let mut conn = state.get_redis_connection().await?;
    let acquired: bool = conn.set_nx(&key, "1").await?;

    if !acquired {
        // Someone else is already generating for this concept — don't
        // call Claude a second time for a result that would just be
        // discarded; wait for theirs and hand back whatever they save.
        return poll_for_canonical(state, concept_id, LOCK_TTL_SECONDS).await;
    }
    conn.expire::<_, ()>(&key, LOCK_TTL_SECONDS as i64).await?;

    // The lock's lifetime must cover generation AND persistence, not
    // just the Dify call — found live (Block 12 verification, a real
    // concurrent-request test): releasing it right after the call
    // returned let a poller observe a PARTIAL insert (some but not all
    // of a multi-template batch already written) and return that
    // incomplete set as if it were the final answer. Held via a
    // closure so every exit path (including `?` on a failed
    // generation) still releases it exactly once, in the right place.
    let outcome = async {
        let templates = ai_client::generate_exercise_template(
            &state.http_client,
            &state.ai_service_url,
            concept_id,
            concept_meta,
            top_chunks,
            batch_children,
        )
        .await?;

        // Discovered live (Block 12 verification): Dify/Claude can
        // propose more than one template for the SAME difficulty in a
        // single batch (e.g. three "basic" candidates) — the DB's
        // unique index correctly keeps only the first as canonical,
        // but looping insert_or_fetch_existing() per ATTEMPT and
        // returning that raw vector meant the response repeated the
        // same canonical row multiple times (one per attempt), while a
        // polling loser's response (fetch_canonical_templates, below)
        // was already the clean, deduplicated set. Re-fetching fresh
        // here instead of returning the per-attempt vector makes both
        // code paths return byte-for-byte the same shape — the actual
        // canonical set, not an attempt log.
        for template in &templates {
            insert_or_fetch_existing(&state.pool, concept_id, template).await?;
        }
        fetch_canonical_templates(&state.pool, concept_id).await
    }
    .await;

    // Best-effort release regardless of outcome — a held lock past its
    // own TTL only ever costs a little extra waiting for a poller, it
    // never causes incorrect data (the DB constraint still holds).
    let _: Result<(), _> = conn.del(&key).await;

    outcome
}

/// The "losing" request's path: no real generation call — waits for
/// the LOCK to be released (signaling the whole batch, not just part
/// of it, is done) before ever reading the database, then hands back
/// whatever the winner saved.
async fn poll_for_canonical(
    state: &AppState,
    concept_id: Uuid,
    timeout_seconds: u64,
) -> Result<Vec<ExerciseTemplate>, ExerciseError> {
    let key = lock_key(concept_id);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        let mut conn = state.get_redis_connection().await?;
        let lock_still_held: bool = conn.exists(&key).await?;
        if !lock_still_held {
            // Winner released it (finished, one way or another) — only
            // NOW is it safe to read, since a released lock is the
            // signal the whole batch (not just part of it) landed.
            let existing = fetch_canonical_templates(&state.pool, concept_id).await?;
            if !existing.is_empty() {
                return Ok(existing);
            }
            // Lock's gone but nothing was ever saved — the winner's
            // own generation failed rather than validation-passed, so
            // there is nothing left to wait for.
            return Err(ExerciseError::GenerationFailed(
                "concurrent generation finished without producing any templates".to_string(),
            ));
        }
        if tokio::time::Instant::now() >= deadline {
            // Fail-open per PRD.md's Validation section — never block
            // the teaching loop on a failed/stalled generation.
            return Err(ExerciseError::GenerationFailed(
                "timed out waiting for a concurrent generation to finish".to_string(),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn fetch_canonical_templates(pool: &PgPool, concept_id: Uuid) -> Result<Vec<ExerciseTemplate>, ExerciseError> {
    let rows: Vec<ExerciseTemplateRow> = sqlx::query_as(
        "SELECT exercise_type, difficulty, template_body, template_params, correct_answer, \
                grader_type, grader_config, tolerance \
         FROM exercises WHERE concept_id = $1 AND is_canonical = TRUE",
    )
    .bind(concept_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(ExerciseTemplateRow::into_template).collect())
}

async fn fetch_one_canonical(
    pool: &PgPool,
    concept_id: Uuid,
    difficulty: &str,
) -> Result<Option<ExerciseTemplate>, ExerciseError> {
    let row: Option<ExerciseTemplateRow> = sqlx::query_as(
        "SELECT exercise_type, difficulty, template_body, template_params, correct_answer, \
                grader_type, grader_config, tolerance \
         FROM exercises WHERE concept_id = $1 AND difficulty = $2 AND is_canonical = TRUE",
    )
    .bind(concept_id)
    .bind(difficulty)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(ExerciseTemplateRow::into_template))
}

#[derive(sqlx::FromRow)]
struct ExerciseTemplateRow {
    exercise_type: String,
    difficulty: String,
    template_body: serde_json::Value,
    template_params: Option<serde_json::Value>,
    correct_answer: Option<String>,
    grader_type: Option<String>,
    grader_config: Option<serde_json::Value>,
    tolerance: Option<f32>,
}

impl ExerciseTemplateRow {
    fn into_template(self) -> ExerciseTemplate {
        ExerciseTemplate {
            exercise_type: self.exercise_type,
            difficulty: self.difficulty,
            template_body: self.template_body,
            template_params: self.template_params,
            correct_answer: self.correct_answer,
            grader_type: self.grader_type,
            grader_config: self.grader_config,
            tolerance: self.tolerance,
        }
    }
}

/// Inserts one canonical template; on a unique-violation (the
/// correctness layer catching a race the lock didn't), fetches and
/// returns whatever the winner already saved instead of erroring —
/// same graceful-catch pattern as signup_complete's email uniqueness
/// check (auth/handlers.rs).
async fn insert_or_fetch_existing(
    pool: &PgPool,
    concept_id: Uuid,
    template: &ExerciseTemplate,
) -> Result<ExerciseTemplate, ExerciseError> {
    let result = sqlx::query(
        "INSERT INTO exercises \
         (concept_id, exercise_type, difficulty, template_body, template_params, \
          correct_answer, grader_type, grader_config, tolerance, is_canonical) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, TRUE)",
    )
    .bind(concept_id)
    .bind(&template.exercise_type)
    .bind(&template.difficulty)
    .bind(&template.template_body)
    .bind(&template.template_params)
    .bind(&template.correct_answer)
    .bind(&template.grader_type)
    .bind(&template.grader_config)
    .bind(template.tolerance)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(template.clone()),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            fetch_one_canonical(pool, concept_id, &template.difficulty)
                .await?
                .ok_or(ExerciseError::Internal)
        }
        Err(err) => Err(err.into()),
    }
}
