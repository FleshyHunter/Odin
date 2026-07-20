// Thin HTTP client -> FastAPI only (DESIGN.md's locked repository
// structure). Per the AI Gateway Pattern (ARCHITECTURE.md): Rust NEVER
// calls Ollama or embeddings directly — FastAPI is the single AI
// boundary, and this module is the only thing in the Rust backend
// allowed to reach across that boundary. One function per FastAPI
// endpoint as later blocks add them (transcribe, acquire, ...); Block 5
// added /embed, Block 6 adds /generate.

use serde::{Deserialize, Serialize};

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
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

/// Calls FastAPI's POST /generate (Block 6) with a prompt and returns
/// qwen's generated response text.
pub async fn generate(
    client: &reqwest::Client,
    ai_service_url: &str,
    prompt: String,
) -> Result<String, AiClientError> {
    let url = format!("{ai_service_url}/generate");

    let response = client
        .post(&url)
        .json(&GenerateRequest { prompt })
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
