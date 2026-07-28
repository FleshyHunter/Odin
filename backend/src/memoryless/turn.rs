// Chat-turn orchestration: receive one raw message -> analyze_input
// (Block 8) -> STREAM a generate() reply (Blocks 5/6, Block 11 follow-up
// — see markdown/deferred.md #20) -> append both message sides plus one
// audit event to the staged thread once the stream ends. Deliberately
// BARE, per the negotiated Block 11 scope: no ChromaDB retrieval, no
// mastery_bank reads/writes, no prerequisite checks, no completion-check
// logic — memoryless mode has no journey/subject to scope any of that
// against yet.
//
// Streaming architecture: the returned mpsc::Receiver feeds the HTTP
// SSE response (handlers.rs); the actual generation + persistence runs
// in a SEPARATE spawned task, not tied to the SSE response's own
// lifetime. This split matters for cancellation: if the client
// disconnects, axum drops the SSE stream, which drops the Receiver,
// which makes the spawned task's next `tx.send()` fail — that failure
// IS the cancellation signal (stop pulling further chunks from
// ai_service, releasing Ollama/GPU promptly). But the spawned task
// itself keeps running past that point, just long enough to persist
// whatever was generated so far — if persistence lived inside the same
// future that gets dropped on disconnect, a cancelled turn would never
// get saved at all, contradicting the industrial (Claude/ChatGPT)
// behavior of keeping partial text rather than discarding it.

use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::errors::MemorylessError;
use super::staging::{self, StagedAuditEvent, StagedMessage, StagedThread};
use crate::ai_client;
use crate::state::AppState;

// Matches ai_service/app/generation/service.py's MODEL_NAME literal.
// /generate's response carries no model field of its own to read this
// back from, so it's recorded here rather than invented as a guess.
const MODEL_USED: &str = "qwen3.5:9b";

// No longer a whole-request bound (streaming can legitimately run much
// longer in total, as long as output keeps arriving) — this is now a
// PER-CHUNK inactivity timeout: resets every time a delta arrives, only
// trips on true silence (a stalled/dead generation), never on a
// slow-but-actively-thinking one. Same 120s value as the old
// http_client-level timeout, repurposed rather than invented fresh.
const CHUNK_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);

/// Starts a streaming chat turn. Runs analyze_input and opens the
/// generate() stream synchronously (so an unreachable ai_service still
/// surfaces as a normal, immediate error to the caller — see
/// ai_client::generate_stream's own doc comment) — only once that
/// succeeds does this spawn the background task and return the
/// receiving end for the caller to relay onward as SSE.
pub async fn start_turn_stream(
    state: &AppState,
    thread: StagedThread,
    raw_input: String,
    think: bool,
) -> Result<mpsc::Receiver<String>, MemorylessError> {
    // known_terms empty, current_concept_id None: no journey/subject
    // exists in memoryless mode to scope either against (ai_client::
    // analyze_input's own doc comment — this is exactly its "no journey
    // context" case).
    let analysis = ai_client::analyze_input(
        &state.http_client,
        &state.ai_service_url,
        raw_input.clone(),
        Vec::new(),
        None,
    )
    .await?;

    // BARE call — no retrieval/"use only provided material" framing,
    // since no foundation exists yet to retrieve FROM (flagged and
    // agreed as a real consequence of Block 11's scope).
    let chunks = ai_client::generate_stream(
        &state.http_client,
        &state.ai_service_url,
        analysis.cleaned_query.clone(),
        think,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<String>(16);
    let state = state.clone();
    let cleaned_query = analysis.cleaned_query;
    let matched_concepts = analysis.matched_concepts;
    let detected_intent = analysis.detected_intent;
    let thread_id = thread.thread_id;

    tokio::spawn(async move {
        let mut thread = thread;
        let mut accumulated = String::new();
        let mut cutoff_reason: Option<String> = None;

        // Pinned HERE, inside the spawned task's own stack frame — not
        // in start_turn_stream's, which returns (and so its frame goes
        // away) right after this task is spawned.
        // Scoped block: `chunks`'s owned storage (and so the underlying
        // ai_service connection) is dropped the moment this block ends
        // — i.e. immediately on whichever `break` fires below, not only
        // once the whole spawned task eventually finishes. That's what
        // actually releases ai_service/Ollama promptly on cancellation,
        // rather than merely dropping a pinned reference to it.
        {
            tokio::pin!(chunks);

            loop {
                match tokio::time::timeout(CHUNK_INACTIVITY_TIMEOUT, chunks.next()).await {
                    Ok(Some(Ok(delta))) => {
                        accumulated.push_str(&delta);
                        // A failed send means the SSE receiver was
                        // dropped — the client disconnected (or hit
                        // pause). Stop pulling further chunks
                        // immediately, but keep running past this point
                        // to still persist whatever was generated so far.
                        if tx.send(delta).await.is_err() {
                            cutoff_reason = Some("cancelled by user".to_string());
                            break;
                        }
                    }
                    Ok(Some(Err(err))) => {
                        cutoff_reason = Some(format!("ai service error mid-stream: {err}"));
                        break;
                    }
                    Ok(None) => break, // stream ended normally — full response received
                    Err(_) => {
                        cutoff_reason = Some(format!(
                            "generation stalled — no output for {}s",
                            CHUNK_INACTIVITY_TIMEOUT.as_secs()
                        ));
                        break;
                    }
                }
            }
        }

        let now = Utc::now();
        thread.messages.push(StagedMessage {
            role: "user".to_string(),
            content: raw_input.clone(),
            timestamp: now,
        });
        thread.messages.push(StagedMessage {
            role: "tutor".to_string(),
            content: accumulated.clone(),
            timestamp: now,
        });
        // One audit event per turn (Rule 4: mandatory every turn, all
        // modes) regardless of how it ended — staged here rather than
        // written to audit_logs directly, per Rule 51.
        thread.audit_events.push(StagedAuditEvent {
            user_input: raw_input,
            cleaned_query,
            matched_concepts,
            detected_intent,
            response_text: accumulated,
            model_used: MODEL_USED.to_string(),
            error: cutoff_reason,
            timestamp: now,
        });

        if let Err(err) = staging::save(&state, &thread).await {
            tracing::error!(?err, %thread_id, "failed to stage memoryless turn after streaming");
        }
    });

    Ok(rx)
}
