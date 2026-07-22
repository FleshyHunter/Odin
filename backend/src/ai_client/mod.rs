// Thin HTTP client -> FastAPI only (DESIGN.md's locked repository
// structure). Per the AI Gateway Pattern (ARCHITECTURE.md): Rust NEVER
// calls Ollama or embeddings directly — FastAPI is the single AI
// boundary, and this module is the only thing in the Rust backend
// allowed to reach across that boundary. One function per FastAPI
// endpoint as later blocks add them (transcribe, acquire, ...); Block 5
// added /embed, Block 6 adds /generate.

use serde::{Deserialize, Serialize};
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

#[derive(Serialize)]
struct GenerateRequest {
    prompt: String,
    think: bool,
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
        .json(&GenerateRequest { prompt, think })
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
pub async fn analyze_input(
    client: &reqwest::Client,
    ai_service_url: &str,
    text: String,
    known_terms: Vec<String>,
    current_concept_id: Option<Uuid>,
) -> Result<AnalyzeInputResponse, AiClientError> {
    let url = format!("{ai_service_url}/analyze_input");

    let response = client
        .post(&url)
        .json(&AnalyzeInputRequest {
            text,
            known_terms,
            current_concept_id,
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
}
