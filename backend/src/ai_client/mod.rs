// Thin HTTP client -> FastAPI only (DESIGN.md's locked repository
// structure). Per the AI Gateway Pattern (ARCHITECTURE.md): Rust NEVER
// calls Ollama or embeddings directly — FastAPI is the single AI
// boundary, and this module is the only thing in the Rust backend
// allowed to reach across that boundary. One function per FastAPI
// endpoint as later blocks add them (transcribe, acquire, ...); Block 5
// added /embed, Block 6 adds /generate.

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AiClientError {
    // Mirrors AuthError::ServiceUnavailable's reasoning (Rule 29's "no
    // offline mode... a clear failure is more honest than a hang") —
    // an unreachable/slow FastAPI fails this one call clearly rather
    // than blocking indefinitely.
    #[error("AI service unreachable")]
    ServiceUnavailable,
    #[error("AI service returned an unexpected response: {0}")]
    UnexpectedResponse(String),
}

#[derive(Serialize)]
struct EmbedRequest {
    texts: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Calls FastAPI's POST /embed (Block 4) with a batch of texts and
/// returns one 384-dim vector per input, in the same order.
pub async fn embed(
    client: &reqwest::Client,
    ai_service_url: &str,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, AiClientError> {
    let url = format!("{ai_service_url}/embed");

    let response = client
        .post(&url)
        .json(&EmbedRequest { texts })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: EmbedResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.embeddings)
}

// "user" | "assistant" — Ollama /api/chat's own role names. Callers
// (memoryless/turn.rs) map their staged "user"/"tutor" role strings to
// this before constructing one. deferred.md #54.
#[derive(Serialize, Clone)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct GenerateRequest {
    prompt: String,
    think: bool,
    #[serde(default)]
    history: Vec<HistoryMessage>,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

/// Calls FastAPI's POST /generate (Block 6) with a prompt and returns
/// qwen's generated response text. `think` defaults to `true` at call
/// sites for now (reasoning depth was the actual point of adopting this
/// model) — once Block 8's intent classification exists, callers can
/// pass a per-message decision instead of a blanket default.
pub async fn generate(
    client: &reqwest::Client,
    ai_service_url: &str,
    prompt: String,
    think: bool,
) -> Result<String, AiClientError> {
    let url = format!("{ai_service_url}/generate");

    let response = client
        .post(&url)
        .json(&GenerateRequest { prompt, think, history: Vec::new() })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: GenerateResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.response)
}

// One line of ai_service's POST /generate/stream NDJSON body — either a
// text delta or (as the last line, if generation fails partway) an
// error message. Untagged: the two variants have no shared discriminator
// field, so serde picks whichever one actually matches the JSON shape.
#[derive(Deserialize)]
#[serde(untagged)]
enum GenerateStreamLine {
    Delta { delta: String },
    Error { error: String },
}

/// Calls FastAPI's POST /generate/stream (Block 11 follow-up,
/// markdown/deferred.md #20) and returns a stream of text deltas as
/// qwen produces them, instead of blocking for the full response like
/// generate() does. The OUTER Result covers "could the request even be
/// started" (mirrors generate()'s own error handling, checked BEFORE
/// any streaming begins, so an unreachable ai_service still surfaces as
/// a normal, immediate error rather than a stream that opens then
/// immediately fails). The INNER per-item Result covers a failure
/// DURING streaming — a transport error reading the body, or
/// ai_service's own {"error": ...} sentinel line.
///
/// Callers own deciding what "the stream ended early" means (client
/// disconnect, a stalled-too-long gap, an inner Err) — this function
/// only relays what ai_service sent, it doesn't interpret it.
///
/// `history` (deferred.md #54): prior turns to send ahead of `prompt`,
/// oldest first, already role-mapped to Ollama's "user"/"assistant"
/// naming — empty for a turn with no prior context to include.
pub async fn generate_stream(
    client: &reqwest::Client,
    ai_service_url: &str,
    prompt: String,
    think: bool,
    history: Vec<HistoryMessage>,
) -> Result<impl Stream<Item = Result<String, AiClientError>> + Send + 'static, AiClientError> {
    let url = format!("{ai_service_url}/generate/stream");

    let response = client
        .post(&url)
        .json(&GenerateRequest { prompt, think, history })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    // bytes_stream() -> io::Read adapter -> buffered line reader, so
    // NDJSON lines can be pulled one at a time regardless of how the
    // underlying TCP chunks happen to align with them.
    let byte_stream = response
        .bytes_stream()
        .map(|result| result.map_err(std::io::Error::other));
    let reader = tokio_util::io::StreamReader::new(byte_stream);
    let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(reader));

    Ok(async_stream::stream! {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<GenerateStreamLine>(&line) {
                        Ok(GenerateStreamLine::Delta { delta }) => yield Ok(delta),
                        Ok(GenerateStreamLine::Error { error }) => {
                            yield Err(AiClientError::UnexpectedResponse(error));
                            break;
                        }
                        Err(err) => {
                            yield Err(AiClientError::UnexpectedResponse(err.to_string()));
                            break;
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    yield Err(AiClientError::ServiceUnavailable);
                    break;
                }
            }
        }
    })
}

#[derive(Deserialize)]
struct TranscribeResponse {
    text: String,
}

/// Calls FastAPI's POST /transcribe (Block 7) with raw audio bytes and
/// returns the transcribed text. First non-JSON request this module
/// makes — a multipart file upload, not a JSON body. `filename` is
/// forwarded as-is (not hardcoded to "audio.webm") — the FastAPI side
/// derives its temp-file suffix from this, and a mismatched extension
/// on real (non-WebM) audio is exactly the bug already found and fixed
/// once on that side; hardcoding it here would silently reintroduce
/// the same class of bug from the Rust side instead.
pub async fn transcribe(
    client: &reqwest::Client,
    ai_service_url: &str,
    audio_bytes: Vec<u8>,
    filename: &str,
) -> Result<String, AiClientError> {
    let url = format!("{ai_service_url}/transcribe");

    let part = reqwest::multipart::Part::bytes(audio_bytes).file_name(filename.to_string());
    let form = reqwest::multipart::Form::new().part("file", part);

    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: TranscribeResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.text)
}

#[derive(Serialize)]
struct AnalyzeInputRequest {
    text: String,
    known_terms: Vec<String>,
    current_concept_id: Option<Uuid>,
    // deferred.md #75/2b — only meaningful alongside current_concept_id;
    // together they let ai_service look up that concept's own stored
    // embedding for a richer on-topic signal than word-match alone.
    subject_id: Option<Uuid>,
    dag_version: Option<i32>,
    // deferred.md #2b — display names for Stage 2's classify_gap()
    // prompt if the ambiguous band is ever reached. Independent of
    // subject_id/dag_version above: pass whenever available, None is
    // safe (Stage 2 just stays unreachable, same as missing IDs).
    subject_title: Option<String>,
    current_concept_title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeInputResponse {
    pub raw_input: String,
    pub cleaned_query: String,
    pub lemmas: Vec<String>,
    pub keywords: Vec<String>,
    pub is_on_topic: bool,
    pub matched_concepts: Vec<String>,
    pub detected_intent: String,
    // deferred.md #2b — "on_topic_elsewhere" | "off_topic" | "ambiguous"
    // | "dag_gap" | None. Additive only, not yet consumed by anything
    // (2c/2d are the real callers) — see this fn's own doc comment.
    pub gap_classification: Option<String>,
}

/// Calls FastAPI's POST /analyze_input (Block 8) — the 6-step
/// normalization/intent pipeline (shorthand expansion, spellcheck,
/// domain fuzzy-match, spaCy, intent classification) — and returns the
/// full result. Unlike embed()/generate()/transcribe(), which each
/// return one scalar, this returns the whole response struct: the
/// pipeline's response has 7 fields and callers will generally need
/// more than one of them.
///
/// known_terms must already be subject-scoped (the current journey's
/// subject's concepts only, not the full cross-subject vocabulary
/// bank) — this function does not query the database itself, matching
/// the thin-wrapper shape of the other three; resolving known_terms
/// from canonical_concepts/concept_aliases is the caller's job.
/// current_concept_id is presence-only from ai_service's side (it's
/// never parsed or validated there) — Some(_) signals "mid-journey"
/// for the TANGENT vs OUT_OF_SCOPE split, None signals no journey
/// context.
///
/// subject_id/dag_version (deferred.md #75/2b): only meaningful
/// alongside current_concept_id — pass all three or none. memoryless/
/// turn.rs passes None for all three, same as before; journeys/turn.rs
/// already has subject_id/dag_version in scope right where it already
/// calls this (both were being fetched for other reasons — known_terms
/// itself needs dag_version already), so this is free at that call
/// site, not a new lookup.
///
/// subject_title/current_concept_title (deferred.md #2b): display
/// names for ai_service's Stage 2 classify_gap() prompt, only ever used
/// if the ambiguous band is reached. memoryless/turn.rs passes None for
/// both, same as subject_id/dag_version.
#[allow(clippy::too_many_arguments)]
pub async fn analyze_input(
    client: &reqwest::Client,
    ai_service_url: &str,
    text: String,
    known_terms: Vec<String>,
    current_concept_id: Option<Uuid>,
    subject_id: Option<Uuid>,
    dag_version: Option<i32>,
    subject_title: Option<String>,
    current_concept_title: Option<String>,
) -> Result<AnalyzeInputResponse, AiClientError> {
    let url = format!("{ai_service_url}/analyze_input");

    let response = client
        .post(&url)
        .json(&AnalyzeInputRequest {
            text,
            known_terms,
            current_concept_id,
            subject_id,
            dag_version,
            subject_title,
            current_concept_title,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: AnalyzeInputResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body)
}

#[derive(Serialize)]
struct GradeRequest {
    exercise_type: String,
    student_answer: String,
    correct_answer: Option<String>,
    tolerance: Option<f32>,
    grader_config: Option<Json>,
}

#[derive(Debug, Deserialize)]
pub struct GradeResponse {
    pub is_correct: bool,
    pub score: f32,
    pub feedback: Option<String>,
}

/// Calls FastAPI's POST /grade (Block 9) — deterministic grading for
/// all 5 exercise types (mcq/numeric/symbolic_math/fill_blank/
/// short_answer). No LLM call anywhere in this path (Rule 2/Rule 50) —
/// unlike analyze_input's qwen fallback, this has no network
/// dependency beyond FastAPI itself, so its real end-to-end test below
/// is fully deterministic.
///
/// correct_answer/tolerance/grader_config map directly onto the
/// existing Exercise model's own fields (models/assessment.rs) — the
/// caller's job is reading these off the exercises row (or
/// quiz_attempts.expected_answer for the per-instance correct value)
/// and forwarding them; this function does not query the database
/// itself, same thin-wrapper shape as the other four ai_client
/// functions.
pub async fn grade(
    client: &reqwest::Client,
    ai_service_url: &str,
    exercise_type: String,
    student_answer: String,
    correct_answer: Option<String>,
    tolerance: Option<f32>,
    grader_config: Option<Json>,
) -> Result<GradeResponse, AiClientError> {
    let url = format!("{ai_service_url}/grade");

    let response = client
        .post(&url)
        .json(&GradeRequest {
            exercise_type,
            student_answer,
            correct_answer,
            tolerance,
            grader_config,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: GradeResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body)
}

// ===== Block 10: AcquisitionProvider (Dify) =====
//
// UNVERIFIED against a real Dify instance as of this build — no Dify
// account exists yet (confirmed with the user, who asked for this to
// be built ahead of setup rather than blocked on it). Every function
// below compiles, type-checks, and its request/response shape matches
// ai_service's real Pydantic models — but the actual Dify workflow
// contract (ai_service/app/acquisition/dify_client.py's own
// documented assumption) has never been exercised against a live
// Dify app. Re-verify this whole section first once Dify is set up
// (see setup instructions) — same AI-gateway boundary as everywhere
// else: Rust never calls Dify directly, only through these three
// ai_service endpoints.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Resource {
    pub title: String,
    pub url: Option<String>,
    pub author_org: Option<String>,
    pub content: String,
    pub license_status: Option<String>,
}

#[derive(Serialize)]
struct AcquireRequest {
    topic: String,
}

#[derive(Deserialize)]
struct AcquireResponse {
    resources: Vec<Resource>,
}

/// Calls FastAPI's POST /acquire (Block 10) — Dify -> Gemini, web
/// search + retrieval grounding (Flow 5's acquisition fallback).
/// Returns RAW acquired content, not yet chunked/embedded — the
/// caller reuses Flow 1's existing ingestion pipeline (chunk + POST
/// /embed) for that, same as any other web_fetch source.
pub async fn acquire(
    client: &reqwest::Client,
    ai_service_url: &str,
    topic: String,
) -> Result<Vec<Resource>, AiClientError> {
    let url = format!("{ai_service_url}/acquire");

    let response = client
        .post(&url)
        .json(&AcquireRequest { topic })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: AcquireResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.resources)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntakeContext {
    pub level: String,
    pub goal: String,
    pub background: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConceptNode {
    pub title: String,
    pub description: String,
    pub difficulty_level: i32,
    pub learning_objective: Option<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExerciseTemplate {
    pub exercise_type: String,
    pub difficulty: String,
    pub template_body: Json,
    pub template_params: Option<Json>,
    pub correct_answer: Option<String>,
    pub grader_type: Option<String>,
    pub grader_config: Option<Json>,
    pub tolerance: Option<f32>,
}

#[derive(Serialize)]
struct GenerateDagRequest {
    topic: String,
    intake_context: Option<IntakeContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DAGResult {
    pub concepts: Vec<ConceptNode>,
    pub entry_concept: String,
    pub diagnostic_primary: Option<ExerciseTemplate>,
    pub diagnostic_backup: Option<ExerciseTemplate>,
}

// ============================================================
// TEMPORARY — Dify is currently out of credits (2026-08-08), not a code
// or infra problem. This lets #4/#40's wiring actually be built and
// exercised end-to-end without a live Dify account. REMOVE ONCE DIFY
// CREDITS ARE RESTORED — never meant to ship enabled; default is off.
// Scoped to the Dify-backed calls that need a real DAG-shaped result to
// keep their callers exercisable: generate_dag()/adjust_dag()/
// generate_exercise_template() (Onboarding Diagnostic flow — the last one
// only via journeys::service's existing-subject reuse path,
// exercises/service.rs::get_or_generate) and fold_concept_into_dag()
// (deferred.md #2c, journeys/turn.rs's live turn loop). Does not touch
// ai_service, and does not affect acquire()/classify_gap() at all — both
// already fail soft on a Dify error with zero loss of caller
// exercisability (classify_gap()'s caller just stays "ambiguous").
// ============================================================
fn mock_dify_enabled() -> bool {
    std::env::var("AI_CLIENT_MOCK_DIFY").map(|v| v == "true").unwrap_or(false)
}

/// Synthetic but structurally valid DAGResult: 3 concepts of increasing
/// difficulty_level with a real prerequisite chain (passes #6's own
/// dangling-reference validation shape), an entry_concept chosen from
/// intake_context.level when given (matches level_to_difficulty()'s own
/// three tiers) or the most basic concept when None (the skip case).
/// Both diagnostic exercises are real, instantiable, gradeable
/// symbolic_math templates — arithmetic simple enough that the real
/// deterministic grader (unaffected by Dify being down) actually works
/// against them end-to-end.
fn mock_dag_result(intake_context: &Option<IntakeContext>) -> DAGResult {
    let concepts = vec![
        ConceptNode {
            title: "Basic Arithmetic".to_string(),
            description: "Addition and subtraction of whole numbers.".to_string(),
            difficulty_level: 1,
            learning_objective: Some("Add and subtract small whole numbers.".to_string()),
            prerequisites: vec![],
        },
        ConceptNode {
            title: "Linear Equations".to_string(),
            description: "Solving equations of the form ax + b = c.".to_string(),
            difficulty_level: 2,
            learning_objective: Some("Isolate a variable in a one-step linear equation.".to_string()),
            prerequisites: vec!["Basic Arithmetic".to_string()],
        },
        ConceptNode {
            title: "Quadratic Equations".to_string(),
            description: "Solving equations of the form ax^2 + bx + c = 0.".to_string(),
            difficulty_level: 3,
            learning_objective: Some("Factor and solve a simple quadratic equation.".to_string()),
            prerequisites: vec!["Linear Equations".to_string()],
        },
    ];

    let entry_concept = match intake_context.as_ref().map(|i| i.level.trim().to_lowercase()) {
        Some(level) if level == "advanced" => "Quadratic Equations",
        Some(level) if level == "intermediate" => "Linear Equations",
        _ => "Basic Arithmetic",
    }
    .to_string();

    let diagnostic_primary = Some(ExerciseTemplate {
        exercise_type: "symbolic_math".to_string(),
        difficulty: "basic".to_string(),
        template_body: serde_json::json!({ "question_template": "What is {a} + {b}?" }),
        template_params: Some(serde_json::json!({
            "a": { "min": 2, "max": 9 },
            "b": { "min": 2, "max": 9 },
        })),
        correct_answer: Some("{a}+{b}".to_string()),
        grader_type: Some("symbolic_math".to_string()),
        grader_config: None,
        tolerance: None,
    });
    let diagnostic_backup = Some(ExerciseTemplate {
        exercise_type: "symbolic_math".to_string(),
        difficulty: "basic".to_string(),
        template_body: serde_json::json!({ "question_template": "What is {a} - {b}?" }),
        template_params: Some(serde_json::json!({
            "a": { "min": 5, "max": 9 },
            "b": { "min": 1, "max": 4 },
        })),
        correct_answer: Some("{a}-{b}".to_string()),
        grader_type: Some("symbolic_math".to_string()),
        grader_config: None,
        tolerance: None,
    });

    DAGResult {
        concepts,
        entry_concept,
        diagnostic_primary,
        diagnostic_backup,
    }
}

/// Same TEMPORARY mock as mock_dag_result — extends the given draft with
/// one new, more-foundational concept below its current lowest
/// difficulty_level (matches adjust_dag()'s own real contract: extend,
/// never discard), and repoints entry_concept at it.
fn mock_adjust_dag_result(mut draft: DAGResult) -> DAGResult {
    let lowest = draft.concepts.iter().map(|c| c.difficulty_level).min().unwrap_or(2);
    // subject_concepts.difficulty_level has a real CHECK(BETWEEN 1 AND 5)
    // constraint (0001_initial_schema.sql) — found live via this exact
    // mock (2026-08-08): the naive `lowest - 1` produced 0 for a draft
    // already starting at 1, a real constraint violation, not a fake-data
    // quirk. Floored here so the mock stays valid; see deferred.md for
    // whether the REAL adjust_dag()/#6 validation needs its own floor
    // check once Dify is back (this mock can't tell you that — it's not
    // exercising the real Claude-authored response shape).
    let new_difficulty = (lowest - 1).max(1);
    let new_concept = ConceptNode {
        title: "Number Sense".to_string(),
        description: "Counting and comparing whole numbers.".to_string(),
        difficulty_level: new_difficulty,
        learning_objective: Some("Count and compare small whole numbers.".to_string()),
        prerequisites: vec![],
    };
    draft.entry_concept = new_concept.title.clone();
    draft.concepts.push(new_concept);
    draft
}

/// Calls FastAPI's POST /generate_dag (Block 10) — Dify -> Claude
/// (pedagogical concept ordering/prerequisite reasoning). When
/// intake_context is Some (Onboarding Diagnostic, new-subject
/// branch), the SAME call also returns diagnostic_primary/
/// diagnostic_backup — one combined Dify call, not two.
pub async fn generate_dag(
    client: &reqwest::Client,
    ai_service_url: &str,
    topic: String,
    intake_context: Option<IntakeContext>,
) -> Result<DAGResult, AiClientError> {
    if mock_dify_enabled() {
        tracing::warn!(%topic, "AI_CLIENT_MOCK_DIFY enabled — returning a fake DAGResult, not a real Dify call");
        return Ok(mock_dag_result(&intake_context));
    }

    let url = format!("{ai_service_url}/generate_dag");

    let response = client
        .post(&url)
        .json(&GenerateDagRequest {
            topic,
            intake_context,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: DAGResult = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body)
}

#[derive(Serialize)]
struct AdjustDagRequest {
    topic: String,
    draft: DAGResult,
    reason: String,
}

/// Calls FastAPI's POST /adjust_dag (deferred.md #4/#6) — Onboarding
/// Diagnostic Steps 3-4's DAG-repair call, used only when a confirmed
/// downgrade finds no already-present concept in the draft DAG with a
/// lower `difficulty_level` than the current entry concept to fall back
/// to. Reuses the SAME `DIFY_DAG_API_KEY`/Dify app as `generate_dag()` —
/// a different prompt to the same generic prompt-in/JSON-out workflow,
/// not a new Dify app. Extends the existing draft (adds real
/// foundational concepts underneath it) rather than replacing it, so
/// whatever advanced content the student's stated goal still needs
/// isn't discarded.
pub async fn adjust_dag(
    client: &reqwest::Client,
    ai_service_url: &str,
    topic: String,
    draft: DAGResult,
    reason: String,
) -> Result<DAGResult, AiClientError> {
    if mock_dify_enabled() {
        tracing::warn!(%topic, %reason, "AI_CLIENT_MOCK_DIFY enabled — returning a fake adjusted DAGResult, not a real Dify call");
        return Ok(mock_adjust_dag_result(draft));
    }

    let url = format!("{ai_service_url}/adjust_dag");

    let response = client
        .post(&url)
        .json(&AdjustDagRequest { topic, draft, reason })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: DAGResult = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body)
}

/// deferred.md #2c — the ONE new concept to fold into a journey's
/// existing DAG. Mirrors ConceptNode's own title-based prerequisites
/// convention (the caller resolves titles back to concept_ids).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldedConcept {
    pub title: String,
    pub description: String,
    pub difficulty_level: i32,
    pub learning_objective: Option<String>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
}

/// Synthetic but structurally valid FoldedConcept — same TEMPORARY mock
/// posture as mock_dag_result/mock_adjust_dag_result (AI_CLIENT_MOCK_DIFY,
/// Dify out of credits). prerequisites names current_concept_title
/// verbatim, matching what a real fold response would almost always do.
fn mock_folded_concept(current_concept_title: &str) -> FoldedConcept {
    FoldedConcept {
        title: format!("{current_concept_title} — related detail"),
        description: "A small, related concept the student asked about mid-conversation.".to_string(),
        difficulty_level: 2,
        learning_objective: Some("Understand this related detail well enough to connect it back to the main path.".to_string()),
        prerequisites: vec![current_concept_title.to_string()],
    }
}

#[derive(Serialize)]
struct FoldConceptRequest {
    subject_title: String,
    concept_titles: Vec<String>,
    current_concept_title: String,
    reply_text: String,
}

/// Calls FastAPI's POST /fold_concept (deferred.md #2c) — Dify -> Claude
/// designs ONE new concept to extend a journey's DAG with, for the
/// "fold_gap" classification specifically. Reuses the SAME
/// AI_CLIENT_MOCK_DIFY posture as generate_dag()/adjust_dag() — this is
/// a DAG-authoring call against the same generic-passthrough Dify app
/// (see ai_service's own fold_concept_into_dag() comment for why it
/// reuses DIFY_DAG_API_KEY, not a new key).
pub async fn fold_concept_into_dag(
    client: &reqwest::Client,
    ai_service_url: &str,
    subject_title: String,
    concept_titles: Vec<String>,
    current_concept_title: String,
    reply_text: String,
) -> Result<FoldedConcept, AiClientError> {
    if mock_dify_enabled() {
        tracing::warn!(%current_concept_title, "AI_CLIENT_MOCK_DIFY enabled — returning a fake FoldedConcept, not a real Dify call");
        return Ok(mock_folded_concept(&current_concept_title));
    }

    let url = format!("{ai_service_url}/fold_concept");

    let response = client
        .post(&url)
        .json(&FoldConceptRequest {
            subject_title,
            concept_titles,
            current_concept_title,
            reply_text,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: FoldedConcept = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConceptMeta {
    pub title: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub chunk_type: Option<String>,
    pub difficulty: Option<String>,
}

#[derive(Serialize)]
struct GenerateExerciseTemplateRequest {
    concept_id: Uuid,
    concept_meta: ConceptMeta,
    top_chunks: Vec<Chunk>,
    batch_children: Vec<Uuid>,
}

#[derive(Deserialize)]
struct GenerateExerciseTemplateResponse {
    templates: Vec<ExerciseTemplate>,
}

/// Calls FastAPI's POST /generate_exercise_template (Block 10) — Dify
/// -> Claude (qwen NEVER authors templates). Batches the entry concept
/// along with up to 3 immediate children in ONE call. Returns an EMPTY
/// Vec on validation failure after ai_service's one retry (fail-open,
/// PRD.md: "Never block the teaching loop on a failed template
/// generation") — not an error; the caller is expected to mark the
/// concept template-pending and continue, not treat an empty result as
/// this function having failed.
///
/// Race prevention (the DB unique index + Redis lock,
/// generating_template:{concept_id}) is the CALLER's responsibility —
/// ai_service has no DB/Redis access, so deciding whether to call this
/// at all, and holding the lock around that decision, happens here in
/// Rust, not inside ai_service.
/// Same TEMPORARY reasoning as mock_dag_result (see its own comment) —
/// this path is also Dify-backed and reachable from journeys::service's
/// existing-subject reuse branch (get_or_generate_canonical_exercise).
/// One template per difficulty tier, so a caller filtering by any of the
/// three real difficulty strings finds a match.
fn mock_exercise_templates() -> Vec<ExerciseTemplate> {
    ["basic", "intermediate", "advanced"]
        .iter()
        .map(|difficulty| ExerciseTemplate {
            exercise_type: "symbolic_math".to_string(),
            difficulty: difficulty.to_string(),
            template_body: serde_json::json!({ "question_template": "What is {a} + {b}?" }),
            template_params: Some(serde_json::json!({
                "a": { "min": 2, "max": 9 },
                "b": { "min": 2, "max": 9 },
            })),
            correct_answer: Some("{a}+{b}".to_string()),
            grader_type: Some("symbolic_math".to_string()),
            grader_config: None,
            tolerance: None,
        })
        .collect()
}

pub async fn generate_exercise_template(
    client: &reqwest::Client,
    ai_service_url: &str,
    concept_id: Uuid,
    concept_meta: ConceptMeta,
    top_chunks: Vec<Chunk>,
    batch_children: Vec<Uuid>,
) -> Result<Vec<ExerciseTemplate>, AiClientError> {
    if mock_dify_enabled() {
        tracing::warn!(%concept_id, "AI_CLIENT_MOCK_DIFY enabled — returning fake ExerciseTemplates, not a real Dify call");
        return Ok(mock_exercise_templates());
    }

    let url = format!("{ai_service_url}/generate_exercise_template");

    let response = client
        .post(&url)
        .json(&GenerateExerciseTemplateRequest {
            concept_id,
            concept_meta,
            top_chunks,
            batch_children,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: GenerateExerciseTemplateResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.templates)
}

#[derive(Serialize)]
struct InstantiateExerciseRequest {
    exercise_type: String,
    template_body: Json,
    template_params: Option<Json>,
    correct_answer: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InstantiatedExercise {
    pub rendered_question: String,
    pub instantiated_params: Json,
    pub expected_answer: Option<String>,
    pub rendered_choices: Option<Vec<String>>,
}

/// Calls FastAPI's POST /instantiate_exercise — draws real random
/// values and fills a canonical exercise's templates with them,
/// producing an actual servable question (not a validation dry run).
/// Deliberately NOT part of the acquisition/Dify family — this is pure
/// deterministic computation (no LLM, no Dify), so it's a plain,
/// unmocked HTTP call even when AI_CLIENT_MOCK_DIFY is set.
pub async fn instantiate_exercise(
    client: &reqwest::Client,
    ai_service_url: &str,
    exercise_type: String,
    template_body: Json,
    template_params: Option<Json>,
    correct_answer: Option<String>,
) -> Result<InstantiatedExercise, AiClientError> {
    let url = format!("{ai_service_url}/instantiate_exercise");

    let response = client
        .post(&url)
        .json(&InstantiateExerciseRequest {
            exercise_type,
            template_body,
            template_params,
            correct_answer,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))
}

#[derive(Debug, Deserialize, Clone)]
pub struct IngestChunk {
    pub text: String,
    pub token_count: i32,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
pub struct IngestResponse {
    pub extracted_text: String,
    pub rejected: bool,
    pub rejection_reason: Option<String>,
    pub chunks: Vec<IngestChunk>,
}

/// Calls FastAPI's POST /ingest (Block 12 — markdown/deferred.md #21/
/// #23) with a raw uploaded file and the caller-chosen role. A
/// multipart request, same shape as transcribe() — `filename` forwarded
/// as-is so ai_service can branch on its extension (PDF/DOCX/TXT/PNG/
/// HEIC). Ephemeral role returns extracted_text only (empty chunks);
/// prompt_upload/material_upload additionally return chunked +
/// embedded content, ready to persist. content_hash/dedup is NOT this
/// function's concern — that runs in Rust, BEFORE this is ever called
/// (Duplicate Upload Prevention), so a caller only reaches this once a
/// hash-match has already been ruled out.
///
/// max_chunks (deferred.md #47): ai_service checks this BEFORE running
/// embed_texts(), not after — the whole point of the fix. The old
/// design had Rust check chunk_count on the RETURNED result, by which
/// point ai_service had already embedded everything; that check is
/// gone now (uploads/handlers.rs), superseded by this.
///
/// subject_id/dag_version (deferred.md #24): only known for journey-mode
/// uploads (uploads/handlers.rs's new "commit immediately" branch) —
/// omitted entirely, not sent as empty strings, for every other caller
/// (memoryless staging, ephemeral), same convention already used for
/// max_chunks-adjacent optional fields elsewhere in this file. When
/// present, ai_service runs a real topic-relevance check against them;
/// when absent, that check is skipped, same as it always has been.
#[allow(clippy::too_many_arguments)]
pub async fn ingest(
    client: &reqwest::Client,
    ai_service_url: &str,
    file_bytes: Vec<u8>,
    filename: &str,
    role: &str,
    max_chunks: usize,
    subject_id: Option<Uuid>,
    dag_version: Option<i32>,
) -> Result<IngestResponse, AiClientError> {
    let url = format!("{ai_service_url}/ingest");

    let part = reqwest::multipart::Part::bytes(file_bytes).file_name(filename.to_string());
    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("role", role.to_string())
        .text("max_chunks", max_chunks.to_string());
    if let Some(subject_id) = subject_id {
        form = form.text("subject_id", subject_id.to_string());
    }
    if let Some(dag_version) = dag_version {
        form = form.text("dag_version", dag_version.to_string());
    }

    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))
}

#[derive(Serialize)]
struct QueryKnowledgeGlobalRequest {
    embedding: Vec<f32>,
    subject_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    top_k: usize,
}

// Deliberately excludes chunk text — ai_service's knowledge_global
// ChromaDB collection never stores it (metadata-only, per
// ARCHITECTURE.md's locked schema); `chunk_id` matches
// `chunks.chunk_id` exactly, so a caller resolves the actual content
// via Postgres, not this response (see backend/src/knowledge/mod.rs).
#[derive(Debug, Deserialize)]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub similarity: f32,
    pub metadata: Json,
}

#[derive(Deserialize)]
struct QueryKnowledgeGlobalResponse {
    chunks: Vec<RetrievedChunk>,
}

/// Calls FastAPI's POST /knowledge/query (deferred.md #18) — metadata-
/// filtered similarity search against the permanent, shared
/// `knowledge_global` ChromaDB collection. `embedding` is a precomputed
/// query vector (callers already have one from their own `/embed` call,
/// or need one anyway) — this never re-embeds. `subject_id`: None
/// searches the whole collection (memoryless mode, no subject context —
/// ARCHITECTURE.md's "cross-subject tangent retrieval"); Some(_) scopes
/// to one subject (journey mode). `journey_id` (deferred.md #18's own
/// privacy gap, fixed 2026-08-12): journey mode's real, ownership-
/// verified journey — server-side, this makes `prompt_upload` results
/// scoped to THIS journey only, never another journey's private
/// uploads on the same subject; `None` for memoryless mode (no journey
/// concept). Results already below `RETRIEVAL_MIN_SCORE` are dropped
/// server-side, and the returned order is `final_rank_score` (deferred.md
/// #64), not raw similarity.
pub async fn query_knowledge_global(
    client: &reqwest::Client,
    ai_service_url: &str,
    embedding: Vec<f32>,
    subject_id: Option<Uuid>,
    journey_id: Option<Uuid>,
    top_k: usize,
) -> Result<Vec<RetrievedChunk>, AiClientError> {
    let url = format!("{ai_service_url}/knowledge/query");

    let response = client
        .post(&url)
        .json(&QueryKnowledgeGlobalRequest { embedding, subject_id, journey_id, top_k })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let body: QueryKnowledgeGlobalResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(body.chunks)
}

// deferred.md #18b — the write half. Field names/shape mirror
// ai_service's own ChunkRecord exactly (ARCHITECTURE.md's ChromaDB
// Collection [LOCKED] schema). chunk_id is a String on the wire (Chroma
// document ids are strings), always a real chunks.chunk_id UUID's
// to_string() from the caller's side.
#[derive(Serialize)]
pub struct ChunkRecord {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
    pub source_id: String,
    pub upload_role: String,
    pub trust_score: f32,
    pub subject_id: Option<String>,
    pub concept_id: Option<String>,
    pub journey_id: Option<String>,
    pub difficulty: Option<String>,
    pub chunk_type: Option<String>,
}

#[derive(Serialize)]
struct AddChunksRequest {
    records: Vec<ChunkRecord>,
}

#[derive(Deserialize)]
struct AddChunksResponse {
    #[allow(dead_code)] // Not consumed by the one real caller yet — the count is informational only.
    added: usize,
}

/// Calls FastAPI's POST /knowledge/add_chunks (deferred.md #18b) —
/// upserts real chunk documents into the permanent, shared
/// `knowledge_global` collection. Fire-and-forget from the caller's own
/// perspective is the RIGHT default here (see
/// memoryless::write_through::write_through_material_upload's own
/// fail-soft handling) — this function itself still surfaces a real
/// `Err` on failure, same as every other ai_client function; it's the
/// caller's job to decide fail-soft vs. fail-hard, not this one.
pub async fn add_chunks(
    client: &reqwest::Client,
    ai_service_url: &str,
    records: Vec<ChunkRecord>,
) -> Result<(), AiClientError> {
    if records.is_empty() {
        return Ok(());
    }
    let url = format!("{ai_service_url}/knowledge/add_chunks");

    let response = client
        .post(&url)
        .json(&AddChunksRequest { records })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let _body: AddChunksResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(())
}

#[derive(Serialize)]
struct AddConceptEmbeddingRequest {
    concept_id: String,
    subject_id: String,
    dag_version: i32,
    title: String,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct AddConceptEmbeddingResponse {
    #[allow(dead_code)] // Informational only, same as AddChunksResponse's own unused `added`.
    added: bool,
}

/// Calls FastAPI's POST /knowledge/add_concept_embedding (deferred.md
/// #75/2b) — populates the `concept_embeddings` collection
/// `add_concept_embedding()` has always been able to write to but
/// nothing has ever called (same "capability before caller" gap #18b
/// closed for `knowledge_global`). One real caller:
/// journeys::service::persist_new_subject, right after a brand-new
/// subject's concepts commit.
pub async fn add_concept_embedding(
    client: &reqwest::Client,
    ai_service_url: &str,
    concept_id: Uuid,
    subject_id: Uuid,
    dag_version: i32,
    title: String,
    embedding: Vec<f32>,
) -> Result<(), AiClientError> {
    let url = format!("{ai_service_url}/knowledge/add_concept_embedding");

    let response = client
        .post(&url)
        .json(&AddConceptEmbeddingRequest {
            concept_id: concept_id.to_string(),
            subject_id: subject_id.to_string(),
            dag_version,
            title,
            embedding,
        })
        .send()
        .await
        .map_err(|_| AiClientError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AiClientError::UnexpectedResponse(format!(
            "status {}",
            response.status()
        )));
    }

    let _body: AddConceptEmbeddingResponse = response
        .json()
        .await
        .map_err(|err| AiClientError::UnexpectedResponse(err.to_string()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Real end-to-end proof against the actual Windows-hosted FastAPI —
    // not a mock, no test server. #[ignore] so a routine `cargo test`
    // (e.g. Windows asleep) doesn't fail on an external dependency;
    // run explicitly with `cargo test -- --ignored --nocapture`.
    use std::time::Duration;

    use super::*;

    const AI_SERVICE_URL: &str = "http://100.125.58.90:8000";

    // Identical construction to main.rs's real AppState.http_client —
    // not reqwest::Client::new(), so this exercises the same
    // configuration production code actually runs with, not a
    // differently-configured stand-in.
    fn production_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build reqwest client")
    }

    #[tokio::test]
    #[ignore]
    async fn embed_real_end_to_end() {
        let client = production_client();
        let result = embed(&client, AI_SERVICE_URL, vec!["hello world".to_string()])
            .await
            .expect("embed() call failed");

        println!("embed() returned {} vector(s)", result.len());
        println!("first vector length: {}", result[0].len());
        println!("first 5 values: {:?}", &result[0][..5]);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 384);
    }

    #[tokio::test]
    #[ignore]
    async fn generate_real_end_to_end() {
        let client = production_client();
        let result = generate(
            &client,
            AI_SERVICE_URL,
            "Say hello in one sentence.".to_string(),
            true,
        )
        .await
        .expect("generate() call failed");

        println!("generate() returned: {result}");

        assert!(!result.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn transcribe_real_end_to_end() {
        // Real recorded speech (icons/test.m4a — a repo asset, borrowed
        // here rather than duplicated into a dedicated fixtures dir),
        // already confirmed via direct curl to transcribe accurately.
        // This is the first time it's gone through the actual Rust
        // ai_client::transcribe() code path, not just FastAPI directly.
        let audio_bytes = std::fs::read("../icons/test.m4a")
            .expect("failed to read ../icons/test.m4a — run from the backend/ crate root");

        let client = production_client();
        let result = transcribe(&client, AI_SERVICE_URL, audio_bytes, "test.m4a")
            .await
            .expect("transcribe() call failed");

        println!("transcribe() returned: {result}");

        assert!(!result.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn analyze_input_real_end_to_end() {
        let client = production_client();
        let result = analyze_input(
            &client,
            AI_SERVICE_URL,
            "what is a matrix".to_string(),
            vec!["matrix".to_string()],
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("analyze_input() call failed");

        println!("analyze_input() returned: {result:?}");

        // Deliberately a message that hits a Step 2 rule ("what is X"
        // -> DEFINITION) rather than one needing the qwen fallback —
        // the fallback's output is model-generated and non-
        // deterministic, which would make an exact-match assertion
        // flaky in a way the rule path doesn't.
        assert_eq!(result.detected_intent, "DEFINITION");
        assert!(result.is_on_topic);
        assert_eq!(result.matched_concepts, vec!["matrix".to_string()]);
    }

    #[tokio::test]
    #[ignore]
    async fn grade_real_end_to_end() {
        // Unlike the other three real end-to-end tests, /grade has NO
        // LLM call anywhere in its path (Rule 2/Rule 50) — fully
        // deterministic, so this can assert an exact result rather
        // than just "non-empty".
        let client = production_client();
        let result = grade(
            &client,
            AI_SERVICE_URL,
            "symbolic_math".to_string(),
            "2*x + 3*x".to_string(),
            Some("5*x".to_string()),
            None,
            None,
        )
        .await
        .expect("grade() call failed");

        println!("grade() returned: {result:?}");

        assert!(result.is_correct);
        assert_eq!(result.score, 1.0);
    }

    // The three tests below are genuinely different from the five
    // above: those are blocked only on Windows being awake and
    // reachable. These are ALSO blocked on Dify not being configured
    // at all yet — a real, currently-true gap, not a hypothetical one.
    // UPDATED: all three Dify apps are now built and confirmed working
    // (verified via direct curl against a local ai_service instance —
    // see problems.md #32/#33). These tests now assert real success,
    // matching the other *_real_end_to_end tests in this file. Kept as
    // #[ignore] for the same reason as the others: needs the real
    // service reachable, not something a routine `cargo test` should
    // depend on. NOTE: AI_SERVICE_URL below points at the
    // Windows-hosted container, not the local Mac venv instance the
    // curl verification actually used — this is the first time these
    // three functions run against Windows specifically, and Windows
    // needs its own .env with the real Dify keys (gitignored, never
    // transferred by git) for this to succeed there.

    #[tokio::test]
    #[ignore]
    async fn acquire_real_end_to_end() {
        let client = production_client();
        let result = acquire(&client, AI_SERVICE_URL, "linear algebra".to_string())
            .await
            .expect("acquire() call failed");

        println!("acquire() returned {} resource(s)", result.len());
        for r in &result {
            println!("  - {}", r.title);
        }

        assert!(!result.is_empty());
        assert!(result.iter().all(|r| !r.title.is_empty() && !r.content.is_empty()));
    }

    #[tokio::test]
    #[ignore]
    async fn generate_dag_real_end_to_end() {
        let client = production_client();
        let result = generate_dag(&client, AI_SERVICE_URL, "linear algebra".to_string(), None)
            .await
            .expect("generate_dag() call failed");

        println!(
            "generate_dag() returned {} concept(s), entry_concept: {}",
            result.concepts.len(),
            result.entry_concept
        );

        assert!(!result.concepts.is_empty());
        assert!(!result.entry_concept.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn generate_exercise_template_real_end_to_end() {
        let client = production_client();
        let result = generate_exercise_template(
            &client,
            AI_SERVICE_URL,
            Uuid::new_v4(),
            ConceptMeta {
                title: "Eigenvalues and Eigenvectors".to_string(),
                description: "An eigenvalue of a square matrix A is a scalar lambda such that Av = lambda*v for some nonzero vector v (the eigenvector).".to_string(),
            },
            vec![Chunk {
                text: "For a 2x2 matrix, the characteristic equation det(A - lambda*I) = 0 gives a quadratic in lambda.".to_string(),
                chunk_type: Some("explanation".to_string()),
                difficulty: Some("intermediate".to_string()),
            }],
            vec![],
        )
        .await
        .expect("generate_exercise_template() call failed");

        println!("generate_exercise_template() returned {} template(s)", result.len());

        assert!(!result.is_empty());
    }
}
