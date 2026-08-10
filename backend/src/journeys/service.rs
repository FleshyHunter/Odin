// Onboarding Diagnostic orchestration (deferred.md #4, #6, #13; PRD.md's
// Onboarding Diagnostic, Steps 1-4). The first real caller of
// generate_dag()/adjust_dag()/grade() and of exercises::service for a
// concept that isn't already part of an existing journey — see
// journeys/mod.rs's own doc comment for why this is "the one remaining
// foundation."

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use redis::AsyncCommands;
use sqlx::PgPool;
use uuid::Uuid;

use crate::ai_client::{self, ConceptMeta, DAGResult, ExerciseTemplate, IntakeContext};
use crate::auth::middleware::begin_rls_transaction;
use crate::exercises::service as exercise_service;
use crate::models::knowledge::Subject;
use crate::state::AppState;

use super::errors::JourneyError;
use super::instantiate;
use super::staging::{self, EntryConcept, StagedDiagnostic, SubjectContext};

fn normalize_topic(topic: &str) -> String {
    topic.trim().to_lowercase()
}

/// PRD.md Step 1's three self-reported Levels map onto `exercises.
/// difficulty`'s own three values — used only for an EXISTING subject's
/// diagnostic (a NEW subject's difficulty scoping happens inside
/// generate_dag() itself, via IntakeContext).
fn level_to_difficulty(level: &str) -> &'static str {
    match level.trim().to_lowercase().as_str() {
        "beginner" => "basic",
        "advanced" => "advanced",
        _ => "intermediate",
    }
}

pub enum StartOutcome {
    Diagnostic {
        diagnostic_id: Uuid,
        question: String,
        exercise_type: String,
        choices: Option<Vec<String>>,
    },
    // PRD.md Step 1's skip path: no diagnostic exercise at all, no Redis
    // staging round-trip — the journey exists immediately.
    JourneyCreated {
        journey_id: Uuid,
    },
}

pub async fn start(
    state: &AppState,
    user_id: Uuid,
    topic: String,
    level: Option<String>,
    goal: Option<String>,
    background: Option<String>,
) -> Result<StartOutcome, JourneyError> {
    let topic = topic.trim().to_string();
    if topic.is_empty() {
        return Err(JourneyError::Validation("topic must not be empty".to_string()));
    }
    let normalized_name = normalize_topic(&topic);

    // Skip is all-or-nothing (matches TrackModal's own UI: the whole
    // Level/Goal/Background section hides together) — either field alone
    // being absent is treated the same as both being absent, rather than
    // silently defaulting the other one.
    let skip = level.is_none() || goal.is_none();

    let existing_subject: Option<Subject> = sqlx::query_as("SELECT * FROM subjects WHERE normalized_name = $1")
        .bind(&normalized_name)
        .fetch_optional(&state.pool)
        .await?;

    if skip {
        let (subject_ctx, entry_concept) = match existing_subject {
            Some(subject) => {
                let entry_concept_id = subject.entry_concept_id.ok_or(JourneyError::Internal)?;
                (
                    SubjectContext::Existing { subject_id: subject.subject_id },
                    EntryConcept::Existing(entry_concept_id),
                )
            }
            None => {
                // No intake_context at all (not a partially-filled one) —
                // per generate_dag()'s own contract, this means no
                // diagnostic gets produced, exactly what skip wants.
                let draft = generate_dag_locked(state, &normalized_name, topic.clone(), None).await?;
                let entry_title = draft.entry_concept.clone();
                (
                    SubjectContext::New {
                        normalized_name: normalized_name.clone(),
                        subject_title: topic.clone(),
                        draft,
                    },
                    EntryConcept::Draft(entry_title),
                )
            }
        };
        let journey_id = finalize(
            state,
            user_id,
            &subject_ctx,
            &entry_concept,
            None,
            None,
            InitialLevelResult::Skipped,
        )
        .await?;
        return Ok(StartOutcome::JourneyCreated { journey_id });
    }

    let level = level.expect("skip is true when level is None");
    let intake = IntakeContext {
        level: level.clone(),
        goal: goal.expect("skip is true when goal is None"),
        background,
    };

    let (subject_ctx, entry_concept, exercise, backup_exercise) = match existing_subject {
        Some(subject) => {
            // #4 audit fix: previously guessed via `ORDER BY order_index
            // LIMIT 1` (raw array position from the original Dify
            // response, unrelated to which concept generate_dag()
            // actually named entry_concept) — only the FIRST student to
            // create a subject got the right one. subjects.entry_concept_id
            // (set once, at persist_new_subject() time) is the real answer.
            let entry_concept_id = subject.entry_concept_id.ok_or(JourneyError::Internal)?;
            let (concept_id, title, description): (Uuid, String, Option<String>) = sqlx::query_as(
                "SELECT cc.concept_id, cc.title, cc.description \
                 FROM canonical_concepts cc \
                 WHERE cc.concept_id = $1",
            )
            .bind(entry_concept_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or(JourneyError::Internal)?;

            let difficulty = level_to_difficulty(&level);
            let template =
                get_or_generate_canonical_exercise(state, concept_id, &title, description, difficulty).await?;
            let exercise = instantiate::instantiate(&template);

            (
                SubjectContext::Existing {
                    subject_id: subject.subject_id,
                },
                EntryConcept::Existing(concept_id),
                exercise,
                None,
            )
        }
        None => {
            let draft = generate_dag_locked(state, &normalized_name, topic.clone(), Some(intake.clone())).await?;
            let primary = draft.diagnostic_primary.clone().ok_or_else(|| {
                JourneyError::GenerationFailed("no diagnostic produced for new subject".to_string())
            })?;
            let backup_exercise = draft.diagnostic_backup.as_ref().map(instantiate::instantiate);
            let exercise = instantiate::instantiate(&primary);
            let entry_title = draft.entry_concept.clone();

            (
                SubjectContext::New {
                    normalized_name: normalized_name.clone(),
                    subject_title: topic.clone(),
                    draft,
                },
                EntryConcept::Draft(entry_title),
                exercise,
                backup_exercise,
            )
        }
    };

    let diagnostic_id = Uuid::new_v4();
    let staged = StagedDiagnostic {
        diagnostic_id,
        user_id,
        topic,
        intake,
        subject: subject_ctx,
        entry_concept,
        exercise: exercise.clone(),
        backup_exercise,
        pending_confirmation: false,
        created_at: Utc::now(),
    };
    staging::save(state, &staged).await?;

    Ok(StartOutcome::Diagnostic {
        diagnostic_id,
        question: exercise.rendered_question,
        exercise_type: exercise.exercise_type,
        choices: exercise.choices,
    })
}

const POLL_INTERVAL: Duration = Duration::from_secs(1);

fn dag_lock_key(normalized_name: &str) -> String {
    format!("generating_dag:{normalized_name}")
}

fn dag_draft_cache_key(normalized_name: &str) -> String {
    format!("generating_dag_result:{normalized_name}")
}

/// #13's race-prevention lock for generate_dag() — the same two-layer
/// pattern already proven in exercises/service.rs (a Redis SET NX cost
/// layer; subjects.normalized_name's own UNIQUE constraint is the real
/// correctness layer, exercised later at finalize()). Unlike #3, nothing
/// is written to Postgres at THIS stage for a poller to read back, so
/// the winner's draft is also cached in Redis, released in cache-then-
/// unlock order so a poller can never observe "lock gone" before the
/// draft it's waiting for exists.
///
/// Accepted limitation (named, not solved — same "measure before
/// optimizing" philosophy as #5): two concurrent /journeys/start calls
/// for the SAME brand-new topic with DIFFERENT self-reported levels both
/// receive the WINNER's diagnostic, scoped to the winner's level. Rare
/// and narrow enough not to engineer around here.
async fn generate_dag_locked(
    state: &AppState,
    normalized_name: &str,
    topic: String,
    // None for PRD.md Step 1's skip path — no intake context at all
    // (not a partially-filled one), which per generate_dag()'s own
    // contract means no diagnostic gets produced, exactly what skip wants.
    intake: Option<IntakeContext>,
) -> Result<DAGResult, JourneyError> {
    let key = dag_lock_key(normalized_name);
    let mut conn = state.get_redis_connection().await?;
    let acquired: bool = conn.set_nx(&key, "1").await?;

    if !acquired {
        return poll_for_dag(state, normalized_name, state.template_gen_lock_ttl_seconds).await;
    }
    conn.expire::<_, ()>(&key, state.template_gen_lock_ttl_seconds as i64).await?;

    let outcome = async {
        let draft = ai_client::generate_dag(&state.http_client, &state.ai_service_url, topic, intake).await?;
        let json = serde_json::to_string(&draft)?;
        conn.set_ex::<_, _, ()>(dag_draft_cache_key(normalized_name), json, state.template_gen_lock_ttl_seconds)
            .await?;
        Ok::<DAGResult, JourneyError>(draft)
    }
    .await;

    let _: Result<(), _> = conn.del(&key).await;
    outcome
}

async fn poll_for_dag(state: &AppState, normalized_name: &str, timeout_seconds: u64) -> Result<DAGResult, JourneyError> {
    let key = dag_lock_key(normalized_name);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        let mut conn = state.get_redis_connection().await?;
        let lock_still_held: bool = conn.exists(&key).await?;
        if !lock_still_held {
            let cached: Option<String> = conn.get(dag_draft_cache_key(normalized_name)).await?;
            return match cached {
                Some(json) => Ok(serde_json::from_str(&json)?),
                None => Err(JourneyError::GenerationFailed(
                    "concurrent DAG generation finished without producing a result".to_string(),
                )),
            };
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(JourneyError::GenerationFailed(
                "timed out waiting for a concurrent DAG generation to finish".to_string(),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// An existing subject's diagnostic is just another canonical exercise
/// template for its entry concept — check for one already saved at the
/// claimed difficulty first (exercises::service::get_or_generate has no
/// such check itself; nothing needed one before this caller), only
/// generating if none exists.
async fn get_or_generate_canonical_exercise(
    state: &AppState,
    concept_id: Uuid,
    title: &str,
    description: Option<String>,
    difficulty: &str,
) -> Result<ExerciseTemplate, JourneyError> {
    if let Some(existing) = exercise_service::fetch_one_canonical(&state.pool, concept_id, difficulty).await? {
        return Ok(existing);
    }
    // No retrieval (#18 not wired) and no batching of child concepts —
    // an explicit cut, not an oversight: this only needs ONE difficulty
    // template for ONE concept, not the fuller cost-optimized batch
    // exercises::service already supports for the normal teaching loop.
    let templates = exercise_service::get_or_generate(
        state,
        concept_id,
        ConceptMeta {
            title: title.to_string(),
            description: description.unwrap_or_default(),
        },
        Vec::new(),
        Vec::new(),
    )
    .await?;
    templates.into_iter().find(|t| t.difficulty == difficulty).ok_or_else(|| {
        JourneyError::GenerationFailed(format!("no {difficulty} template generated for concept {concept_id}"))
    })
}

/// Sent back to the frontend when a contradiction leaves a backup
/// question available — without this, there was no way to render "try
/// the backup question" at all (a bare `backup_available: bool` never
/// carried what it actually asks).
pub struct BackupQuestion {
    pub question: String,
    pub exercise_type: String,
    pub choices: Option<Vec<String>>,
}

pub struct DiagnosticOutcome {
    pub contradicted: bool,
    pub backup_question: Option<BackupQuestion>,
    pub journey_id: Option<Uuid>,
}

pub async fn respond(
    state: &AppState,
    user_id: Uuid,
    diagnostic_id: Uuid,
    answer: String,
) -> Result<DiagnosticOutcome, JourneyError> {
    let mut staged = staging::load_owned(state, diagnostic_id, user_id).await?;
    if staged.pending_confirmation {
        return Err(JourneyError::Validation(
            "diagnostic is already awaiting a downgrade decision".to_string(),
        ));
    }

    let grade = ai_client::grade(
        &state.http_client,
        &state.ai_service_url,
        staged.exercise.exercise_type.clone(),
        answer,
        staged.exercise.correct_answer.clone(),
        staged.exercise.tolerance,
        staged.exercise.grader_config.clone(),
    )
    .await?;

    if grade.is_correct {
        let journey_id = finalize(
            state,
            user_id,
            &staged.subject,
            &staged.entry_concept,
            Some(&staged.intake.level),
            Some(&staged.intake.goal),
            InitialLevelResult::Confirmed,
        )
        .await?;
        staging::delete(state, diagnostic_id).await?;
        return Ok(DiagnosticOutcome {
            contradicted: false,
            backup_question: None,
            journey_id: Some(journey_id),
        });
    }

    let backup_question = staged.backup_exercise.as_ref().map(|b| BackupQuestion {
        question: b.rendered_question.clone(),
        exercise_type: b.exercise_type.clone(),
        choices: b.choices.clone(),
    });
    staged.pending_confirmation = true;
    staging::save(state, &staged).await?;
    Ok(DiagnosticOutcome {
        contradicted: true,
        backup_question,
        journey_id: None,
    })
}

pub async fn retry_backup(
    state: &AppState,
    user_id: Uuid,
    diagnostic_id: Uuid,
    answer: String,
) -> Result<DiagnosticOutcome, JourneyError> {
    let mut staged = staging::load_owned(state, diagnostic_id, user_id).await?;
    if !staged.pending_confirmation {
        return Err(JourneyError::Validation(
            "diagnostic is not awaiting a downgrade decision".to_string(),
        ));
    }
    let backup = staged
        .backup_exercise
        .clone()
        .ok_or_else(|| JourneyError::Validation("no backup exercise available for this diagnostic".to_string()))?;

    let grade = ai_client::grade(
        &state.http_client,
        &state.ai_service_url,
        backup.exercise_type.clone(),
        answer,
        backup.correct_answer.clone(),
        backup.tolerance,
        backup.grader_config.clone(),
    )
    .await?;

    if grade.is_correct {
        // Passing the backup counts as confirmed after all — no downgrade.
        let journey_id = finalize(
            state,
            user_id,
            &staged.subject,
            &staged.entry_concept,
            Some(&staged.intake.level),
            Some(&staged.intake.goal),
            InitialLevelResult::Confirmed,
        )
        .await?;
        staging::delete(state, diagnostic_id).await?;
        return Ok(DiagnosticOutcome {
            contradicted: false,
            backup_question: None,
            journey_id: Some(journey_id),
        });
    }

    confirm_downgrade_inner(state, &mut staged).await?;
    let journey_id = finalize(
        state,
        user_id,
        &staged.subject,
        &staged.entry_concept,
        Some(&staged.intake.level),
        Some(&staged.intake.goal),
        InitialLevelResult::Downgraded,
    )
    .await?;
    staging::delete(state, diagnostic_id).await?;
    Ok(DiagnosticOutcome {
        contradicted: true,
        backup_question: None,
        journey_id: Some(journey_id),
    })
}

pub async fn confirm_downgrade(
    state: &AppState,
    user_id: Uuid,
    diagnostic_id: Uuid,
) -> Result<DiagnosticOutcome, JourneyError> {
    let mut staged = staging::load_owned(state, diagnostic_id, user_id).await?;
    if !staged.pending_confirmation {
        return Err(JourneyError::Validation(
            "diagnostic is not awaiting a downgrade decision".to_string(),
        ));
    }
    confirm_downgrade_inner(state, &mut staged).await?;
    let journey_id = finalize(
        state,
        user_id,
        &staged.subject,
        &staged.entry_concept,
        Some(&staged.intake.level),
        Some(&staged.intake.goal),
        InitialLevelResult::Downgraded,
    )
    .await?;
    staging::delete(state, diagnostic_id).await?;
    Ok(DiagnosticOutcome {
        contradicted: true,
        backup_question: None,
        journey_id: Some(journey_id),
    })
}

/// Decision #3's resolved design: try the free, deterministic fix first
/// (reassign the entry point to an already-present concept in the same
/// draft DAG with a lower difficulty_level) before ever spending a
/// second Dify call. Mutates `staged.subject`/`staged.entry_concept` in
/// place — finalize() reads whatever this leaves behind.
async fn confirm_downgrade_inner(state: &AppState, staged: &mut StagedDiagnostic) -> Result<(), JourneyError> {
    let SubjectContext::New { draft, .. } = &mut staged.subject else {
        // Existing subject: the canonical DAG is never mutated here
        // (locked, shared, versioned) — named KIV per decision #3, not
        // solved. finalize() still flags foundation_gap on this
        // journey's own entry concept; there's simply nothing lower to
        // reassign it to.
        return Ok(());
    };
    let EntryConcept::Draft(current_title) = &staged.entry_concept else {
        // start() never stages the other combination for a New subject.
        return Err(JourneyError::Internal);
    };

    let current_level = draft.concepts.iter().find(|c| &c.title == current_title).map(|c| c.difficulty_level);

    let cheaper_entry = current_level.and_then(|level| {
        draft
            .concepts
            .iter()
            .filter(|c| c.difficulty_level < level)
            .max_by_key(|c| c.difficulty_level)
            .map(|c| c.title.clone())
    });

    if let Some(title) = cheaper_entry {
        staged.entry_concept = EntryConcept::Draft(title);
        return Ok(());
    }

    // Nothing in the draft is foundational enough — the one place this
    // flow spends a second Dify call, and only EXTENDS the draft (see
    // adjust_dag()'s own doc) rather than discarding whatever advanced
    // content the student's stated goal may still need.
    let reason = format!(
        "student self-reported level {} but failed the diagnostic for the current entry concept ({current_title}); the draft has no concept below its difficulty level to fall back to",
        staged.intake.level
    );
    let adjusted =
        ai_client::adjust_dag(&state.http_client, &state.ai_service_url, staged.topic.clone(), draft.clone(), reason)
            .await?;
    let new_entry_title = adjusted.entry_concept.clone();
    *draft = adjusted;
    staged.entry_concept = EntryConcept::Draft(new_entry_title);
    Ok(())
}

#[derive(Clone, Copy)]
enum InitialLevelResult {
    Confirmed,
    Downgraded,
    // PRD.md Step 1's skip path — no diagnostic ran at all, so neither
    // 'confirmed' nor 'downgraded' applies. Writes NULL, not a third
    // string value (the CHECK constraint on initial_level_diagnostic_result
    // only allows the original two — NULL means "not applicable", exactly
    // matching "trust the self-report (or absence of one) as-is").
    Skipped,
}

/// Outcome of `persist_new_subject` — distinguishes "we actually created
/// this subject" from "#13 audit finding: a concurrent request already
/// created it between our Redis lock releasing and this insert running
/// (the lock is a cost optimization only, released well before
/// finalize() — see generate_dag_locked's own doc comment; the real
/// correctness guarantee is subjects.normalized_name's UNIQUE
/// constraint, exercised HERE for the first time)." The loser's own
/// draft content is simply discarded in favor of whatever the winner
/// already persisted — same content either way, since both drafts came
/// from the same Dify call shape for the same topic.
enum PersistOutcome {
    Created {
        subject_id: Uuid,
        dag_version: i32,
        concept_id_by_title: HashMap<String, Uuid>,
    },
    AlreadyExisted {
        subject_id: Uuid,
        dag_version: i32,
        entry_concept_id: Uuid,
    },
}

async fn fetch_subject_concept_rows(
    pool: &PgPool,
    subject_id: Uuid,
    dag_version: i32,
) -> Result<Vec<(Uuid, Option<i32>)>, JourneyError> {
    Ok(
        sqlx::query_as("SELECT concept_id, order_index FROM subject_concepts WHERE subject_id = $1 AND dag_version = $2")
            .bind(subject_id)
            .bind(dag_version)
            .fetch_all(pool)
            .await?,
    )
}

async fn fetch_subject_prereq_pairs(
    pool: &PgPool,
    subject_id: Uuid,
    dag_version: i32,
) -> Result<Vec<(Uuid, Uuid)>, JourneyError> {
    Ok(sqlx::query_as(
        "SELECT concept_id, prereq_concept_id FROM subject_prerequisites WHERE subject_id = $1 AND dag_version = $2",
    )
    .bind(subject_id)
    .bind(dag_version)
    .fetch_all(pool)
    .await?)
}

/// Persists a brand-new subject's canonical content (subjects,
/// canonical_concepts, subject_concepts, subject_prerequisites) — plain
/// pool, no RLS (SCHEMA.md: this content is globally shared, not
/// user-scoped). canonical_concepts rows are inserted fresh every time,
/// deliberately not deduped by title across subjects: the schema has no
/// UNIQUE constraint on canonical_concepts.title, and concept_aliases
/// (a separate, unbuilt mechanism) is the intended home for any future
/// cross-subject concept matching — not naive title comparison here.
async fn persist_new_subject(
    state: &AppState,
    normalized_name: &str,
    subject_title: &str,
    draft: &DAGResult,
) -> Result<PersistOutcome, JourneyError> {
    // Validated BEFORE any write happens (fail fast, no partial state) —
    // #6 already guards `prerequisites` against a dangling title
    // reference; `entry_concept` itself was never checked at all until
    // now. A typo'd entry_concept from Dify is exactly the same class of
    // bug, just never caught for this one field.
    if !draft.concepts.iter().any(|c| c.title == draft.entry_concept) {
        return Err(JourneyError::GenerationFailed(format!(
            "entry_concept {:?} does not match any concept title in the generated DAG",
            draft.entry_concept
        )));
    }

    // Real race window found via live concurrent testing (2026-08-08),
    // not just a theoretical one: this function's writes used to run as
    // separate autocommit statements against the plain pool — INSERT
    // subjects, THEN (several statements later) UPDATE ... SET
    // entry_concept_id. A concurrent request's unique-violation fallback
    // (below) queries this same row and can observe it AFTER the INSERT
    // but BEFORE the UPDATE — a real, transiently-NULL entry_concept_id,
    // not a data-corruption case as the fallback's own comment used to
    // assume. Wrapping the whole function in one transaction makes the
    // insert-everything-then-set-entry_concept_id sequence atomic: a
    // concurrent SELECT now only ever sees this row fully absent (before
    // commit) or fully populated (after), never partial. Also fixes a
    // second real issue this same testing pass hit: any error partway
    // through used to leave orphaned canonical_concepts/subject_concepts
    // rows with no matching subjects.entry_concept_id ever set — now
    // rolls back cleanly instead.
    let mut tx = state.pool.begin().await?;

    let insert_result: Result<Uuid, sqlx::Error> =
        sqlx::query_scalar("INSERT INTO subjects (title, normalized_name) VALUES ($1, $2) RETURNING subject_id")
            .bind(subject_title)
            .bind(normalized_name)
            .fetch_one(&mut *tx)
            .await;

    let subject_id = match insert_result {
        Ok(id) => id,
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            // Our own INSERT lost the race and rolled back nothing (it
            // never committed) — query the WINNER's row on a fresh
            // connection, not `tx` (this transaction has nothing
            // committed in it and is about to be dropped/rolled back).
            let (subject_id, dag_version, entry_concept_id): (Uuid, i32, Option<Uuid>) = sqlx::query_as(
                "SELECT subject_id, dag_version, entry_concept_id FROM subjects WHERE normalized_name = $1",
            )
            .bind(normalized_name)
            .fetch_one(&state.pool)
            .await?;
            // With the transaction fix above, a concurrent winner's row
            // is only ever visible here fully committed — NULL now
            // really would mean something else is wrong.
            let entry_concept_id = entry_concept_id.ok_or(JourneyError::Internal)?;
            return Ok(PersistOutcome::AlreadyExisted { subject_id, dag_version, entry_concept_id });
        }
        Err(err) => return Err(err.into()),
    };
    let dag_version = 1; // subjects.dag_version's own DEFAULT — a brand-new subject always starts here.

    let mut concept_id_by_title = HashMap::new();
    for (index, concept) in draft.concepts.iter().enumerate() {
        let concept_id: Uuid = sqlx::query_scalar(
            "INSERT INTO canonical_concepts (title, description) VALUES ($1, $2) RETURNING concept_id",
        )
        .bind(&concept.title)
        .bind(&concept.description)
        .fetch_one(&mut *tx)
        .await?;
        concept_id_by_title.insert(concept.title.clone(), concept_id);

        sqlx::query(
            "INSERT INTO subject_concepts \
             (subject_id, concept_id, learning_objective, difficulty_level, order_index, generated_by, dag_version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(subject_id)
        .bind(concept_id)
        .bind(&concept.learning_objective)
        .bind(concept.difficulty_level)
        .bind(index as i32)
        .bind("dify_claude")
        .bind(dag_version)
        .execute(&mut *tx)
        .await?;
    }

    for concept in &draft.concepts {
        let Some(&concept_id) = concept_id_by_title.get(&concept.title) else {
            continue;
        };
        for prereq_title in &concept.prerequisites {
            let Some(&prereq_concept_id) = concept_id_by_title.get(prereq_title) else {
                // #6 already guarantees this can't happen for the
                // original draft; defensive only in case adjust_dag()
                // ever lets something slip through in the future.
                tracing::warn!(%prereq_title, "dangling prerequisite reference survived #6 validation, skipping");
                continue;
            };
            sqlx::query(
                "INSERT INTO subject_prerequisites (subject_id, concept_id, prereq_concept_id, strength, dag_version) \
                 VALUES ($1, $2, $3, 'required', $4)",
            )
            .bind(subject_id)
            .bind(concept_id)
            .bind(prereq_concept_id)
            .bind(dag_version)
            .execute(&mut *tx)
            .await?;
        }
    }

    // #4 audit finding: the entry concept a FUTURE student's diagnostic
    // reuses (start()'s existing-subject branch) used to be guessed via
    // order_index (raw array position, unrelated to which concept was
    // actually named entry_concept) — only the first student ever got it
    // right. Recorded durably here, once, at the moment we know the real
    // concept_id (canonical_concepts rows don't exist before this loop).
    let entry_concept_id = *concept_id_by_title
        .get(&draft.entry_concept)
        .expect("validated above: entry_concept matches a concept title");
    sqlx::query("UPDATE subjects SET entry_concept_id = $1 WHERE subject_id = $2")
        .bind(entry_concept_id)
        .bind(subject_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // deferred.md #75/2b: populates concept_embeddings — best-effort,
    // after the real commit above, same "commit real data first, then
    // fail-soft enrichment" shape as write_through_material_upload's
    // own knowledge_global write (#18b). A failure here never blocks
    // subject creation; it only means is_on_topic's richer embedding
    // check has nothing to compare against for these concepts yet,
    // falling back to word-match alone — exactly today's behavior, not
    // a regression. Batch-embeds every concept in ONE call
    // (ai_client::embed already batches), title+description as the
    // concept's semantic representation.
    match ai_client::embed(
        &state.http_client,
        &state.ai_service_url,
        draft.concepts.iter().map(|c| format!("{}. {}", c.title, c.description)).collect(),
    )
    .await
    {
        Ok(embeddings) => {
            for (concept, embedding) in draft.concepts.iter().zip(embeddings) {
                let Some(&concept_id) = concept_id_by_title.get(&concept.title) else { continue };
                if let Err(err) = ai_client::add_concept_embedding(
                    &state.http_client,
                    &state.ai_service_url,
                    concept_id,
                    subject_id,
                    dag_version,
                    concept.title.clone(),
                    embedding,
                )
                .await
                {
                    tracing::warn!(?err, %concept_id, "failed to write concept embedding, continuing");
                }
            }
        }
        Err(err) => {
            tracing::warn!(?err, %subject_id, "failed to batch-embed new subject's concepts, continuing without concept_embeddings");
        }
    }

    Ok(PersistOutcome::Created { subject_id, dag_version, concept_id_by_title })
}

/// Shared by respond()/retry_backup()/confirm_downgrade() (the normal
/// diagnostic path) AND start()'s skip path — the one place a real
/// `journeys` row (and its journey_concepts/journey_prerequisites) ever
/// gets created. Takes the subject/entry-concept context directly rather
/// than a `&StagedDiagnostic`, since the skip path never stages one at
/// all (no diagnostic ran, nothing to stage). `self_report_level`/`goal`
/// are `None` together only for the skip path (PRD.md Step 1: "trust the
/// self-report or absence of one as-is"). Phase 1 (canonical content,
/// only touched for a NEW subject) runs on the plain pool; Phase 2 (this
/// user's own journey) runs inside an RLS transaction, since journeys/
/// journey_concepts ARE RLS-protected unlike subjects/canonical_concepts.
async fn finalize(
    state: &AppState,
    user_id: Uuid,
    subject: &SubjectContext,
    entry_concept: &EntryConcept,
    self_report_level: Option<&str>,
    goal: Option<&str>,
    result: InitialLevelResult,
) -> Result<Uuid, JourneyError> {
    // #4 audit fixes (bugs #1/#2): a New subject can resolve two different
    // ways now — genuinely Created (this student's own draft became
    // canonical), or AlreadyExisted (a concurrent request beat us to it,
    // #13's accepted race — see persist_new_subject's own doc comment).
    // The AlreadyExisted case behaves exactly like a true Existing
    // subject from here on: query concept_rows/prereq_pairs from the DB,
    // and use the SUBJECT's own canonical entry_concept_id rather than
    // this student's now-meaningless staged entry_concept (it names a
    // title from OUR OWN discarded draft, which was never persisted).
    let (subject_id, _dag_version, entry_concept_id, concept_rows, prereq_pairs): (
        Uuid,
        i32,
        Uuid,
        Vec<(Uuid, Option<i32>)>,
        Vec<(Uuid, Uuid)>,
    ) = match subject {
        SubjectContext::Existing { subject_id } => {
            let (dag_version, entry_concept_id): (i32, Option<Uuid>) =
                sqlx::query_as("SELECT dag_version, entry_concept_id FROM subjects WHERE subject_id = $1")
                    .bind(subject_id)
                    .fetch_one(&state.pool)
                    .await?;
            let entry_concept_id = entry_concept_id.ok_or(JourneyError::Internal)?;
            let concept_rows = fetch_subject_concept_rows(&state.pool, *subject_id, dag_version).await?;
            let prereq_pairs = fetch_subject_prereq_pairs(&state.pool, *subject_id, dag_version).await?;
            (*subject_id, dag_version, entry_concept_id, concept_rows, prereq_pairs)
        }
        SubjectContext::New {
            normalized_name,
            subject_title,
            draft,
        } => match persist_new_subject(state, normalized_name, subject_title, draft).await? {
            PersistOutcome::Created { subject_id, dag_version, concept_id_by_title } => {
                let entry_concept_id = match entry_concept {
                    EntryConcept::Existing(id) => *id, // shouldn't happen for a New subject; defensive only.
                    EntryConcept::Draft(title) => {
                        *concept_id_by_title.get(title).ok_or(JourneyError::Internal)?
                    }
                };
                let concept_rows = draft
                    .concepts
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| concept_id_by_title.get(&c.title).map(|&id| (id, Some(i as i32))))
                    .collect();
                let by_title = &concept_id_by_title;
                let prereq_pairs = draft
                    .concepts
                    .iter()
                    .flat_map(move |c| {
                        let concept_id = by_title.get(&c.title).copied();
                        c.prerequisites
                            .iter()
                            .filter_map(move |title| Some((concept_id?, by_title.get(title).copied()?)))
                    })
                    .collect();
                (subject_id, dag_version, entry_concept_id, concept_rows, prereq_pairs)
            }
            PersistOutcome::AlreadyExisted { subject_id, dag_version, entry_concept_id } => {
                let concept_rows = fetch_subject_concept_rows(&state.pool, subject_id, dag_version).await?;
                let prereq_pairs = fetch_subject_prereq_pairs(&state.pool, subject_id, dag_version).await?;
                (subject_id, dag_version, entry_concept_id, concept_rows, prereq_pairs)
            }
        },
    };

    let mut tx = begin_rls_transaction(&state.pool, user_id).await?;

    let initial_level_diagnostic_result: Option<&str> = match result {
        InitialLevelResult::Confirmed => Some("confirmed"),
        InitialLevelResult::Downgraded => Some("downgraded"),
        InitialLevelResult::Skipped => None,
    };

    let journey_id: Uuid = sqlx::query_scalar(
        "INSERT INTO journeys (user_id, subject_id, goal, initial_level_self_report, initial_level_diagnostic_result) \
         VALUES ($1, $2, $3, $4, $5) RETURNING journey_id",
    )
    .bind(user_id)
    .bind(subject_id)
    .bind(goal)
    .bind(self_report_level)
    .bind(initial_level_diagnostic_result)
    .fetch_one(&mut *tx)
    .await?;

    for (concept_id, order_index) in &concept_rows {
        let is_entry = *concept_id == entry_concept_id;
        let status = if is_entry { "available" } else { "locked" };
        let foundation_gap = is_entry && matches!(result, InitialLevelResult::Downgraded);
        sqlx::query(
            "INSERT INTO journey_concepts (journey_id, concept_id, status, order_index, foundation_gap) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(journey_id)
        .bind(concept_id)
        .bind(status)
        .bind(order_index)
        .bind(foundation_gap)
        .execute(&mut *tx)
        .await?;
    }

    for (concept_id, prereq_concept_id) in &prereq_pairs {
        sqlx::query("INSERT INTO journey_prerequisites (journey_id, concept_id, prereq_concept_id) VALUES ($1, $2, $3)")
            .bind(journey_id)
            .bind(concept_id)
            .bind(prereq_concept_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(journey_id)
}
