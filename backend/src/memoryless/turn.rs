// Chat-turn orchestration: receive one raw message -> analyze_input
// (Block 8) -> STREAM a generate() reply (Blocks 5/6, Block 11 follow-up
// — see markdown/deferred.md #20) -> append both message sides plus one
// audit event to the staged thread once the stream ends. Deliberately
// BARE against the permanent knowledge base and journey state, per the
// negotiated Block 11 scope: no ChromaDB retrieval (deferred.md #18,
// still unwired), no mastery_bank reads/writes, no prerequisite checks,
// no completion-check logic — memoryless mode has no journey/subject to
// scope any of that against yet. NOT bare against THIS thread's own
// staged uploads, though (deferred.md #19) — see retrieve_staged_context.
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
use super::similarity::rank_by_similarity;
use super::staging::{self, StagedAuditEvent, StagedMessage, StagedThread, StagedUpload};
use crate::ai_client::{self, AiClientError};
use crate::state::AppState;

// Matches ai_service/app/generation/service.py's MODEL_NAME literal.
// /generate's response carries no model field of its own to read this
// back from, so it's recorded here rather than invented as a guess.
const MODEL_USED: &str = "qwen3.5:9b";

// deferred.md #53: the SSE stream (handlers.rs) previously only ever
// carried plain text deltas — a turn that failed mid-generation just
// ended the stream with zero deltas, byte-identical to an empty-but-
// successful reply. This carries the same `cutoff_reason` this module
// already computes (previously only ever written to the staged audit
// event) out to the client as a real, distinguishable event.
pub enum TurnEvent {
    Delta(String),
    Error(String),
}

// deferred.md #19: memoryless/similarity.rs's cosine-similarity scan,
// built and unit-tested well ahead of this, its first real caller.
// Scoped deliberately to JUST this thread's own staged-upload chunks
// (PRD.md, Staged Upload Retrieval's second of two searches) — the
// other half (a normal query against the permanent global ChromaDB) is
// a separate, still-unwired piece of work (deferred.md #18), since
// nothing today calls ChromaDB from a real chat turn at all. Fails
// open on any embedding-service error: this is a retrieval
// ENHANCEMENT, not core functionality, and the existing bare-query
// behavior without it is already the accepted baseline (deferred.md #18).
async fn retrieve_staged_context(
    state: &AppState,
    query: &str,
    staged_uploads: &[StagedUpload],
) -> Result<Option<String>, AiClientError> {
    let candidates: Vec<(String, Vec<f32>)> = staged_uploads
        .iter()
        .flat_map(|upload| upload.chunks.iter())
        .map(|chunk| (chunk.text.clone(), chunk.embedding.clone()))
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    let query_embedding = ai_client::embed(&state.http_client, &state.ai_service_url, vec![query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AiClientError::UnexpectedResponse("embed() returned no vectors for the query".to_string()))?;

    let relevant: Vec<&str> = rank_by_similarity(&query_embedding, &candidates)
        .into_iter()
        .filter(|(_, score)| *score >= state.retrieval_min_score)
        .map(|(text, _)| text.as_str())
        .collect();

    if relevant.is_empty() {
        return Ok(None);
    }
    Ok(Some(relevant.join("\n\n")))
}

// No longer a whole-request bound (streaming can legitimately run much
// longer in total, as long as output keeps arriving) — this is now a
// PER-CHUNK inactivity timeout: resets every time a delta arrives, only
// trips on true silence (a stalled/dead generation), never on a
// slow-but-actively-thinking one. Same 120s value as the old
// http_client-level timeout, repurposed rather than invented fresh.
const CHUNK_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(120);

// deferred.md #54: previously every turn was generated as if it were
// the first — thread.messages was loaded from Redis but never read
// before calling generate_stream(). 10 messages (5 turn-pairs), plain
// FIFO eviction (oldest dropped first) — both explicitly decided in the
// entry's own planning discussion, sized against OLLAMA_NUM_CTX=8192
// being a hard, shared budget covering system + history + the current
// message + the full generated output (including "thinking" tokens);
// this exact app has already hit silent truncation once before from
// exceeding that budget (see ai_service's own OLLAMA_NUM_CTX comment).
// A plain const, not an env-configurable threshold, matching
// CHUNK_INACTIVITY_TIMEOUT just above — this value came from arithmetic
// against a fixed model/context budget, not an operational knob.
const HISTORY_WINDOW_MESSAGES: usize = 10;

// Builds the capped, role-mapped history generate_stream() sends ahead
// of the current turn's prompt. Only ever reads thread.messages as it
// stood BEFORE this turn's own messages are appended (see the bottom of
// start_turn_stream, well after this is called) — so the current
// student message never ends up duplicated in both history and prompt.
fn build_history(messages: &[StagedMessage]) -> Vec<ai_client::HistoryMessage> {
    let start = messages.len().saturating_sub(HISTORY_WINDOW_MESSAGES);
    messages[start..]
        .iter()
        .map(|message| ai_client::HistoryMessage {
            // StagedMessage.role matches the messages table's own
            // "user"/"tutor" CHECK constraint — Ollama's /api/chat wants
            // "assistant" instead of "tutor"; "user" already matches.
            role: if message.role == "tutor" { "assistant".to_string() } else { message.role.clone() },
            content: message.content.clone(),
        })
        .collect()
}

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
) -> Result<mpsc::Receiver<TurnEvent>, MemorylessError> {
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

    // deferred.md #19: staged-upload retrieval — see
    // retrieve_staged_context's own doc comment. Still BARE against the
    // permanent global knowledge base (deferred.md #18, unchanged) —
    // this only ever looks at chunks staged on THIS thread.
    let retrieved_context = if thread.staged_uploads.iter().any(|upload| !upload.chunks.is_empty()) {
        match retrieve_staged_context(state, &analysis.cleaned_query, &thread.staged_uploads).await {
            Ok(context) => context,
            Err(err) => {
                tracing::warn!(?err, thread_id = %thread.thread_id, "staged-upload retrieval failed, continuing without it");
                None
            }
        }
    } else {
        None
    };

    let prompt = match &retrieved_context {
        Some(context) => format!(
            "The student uploaded a document earlier in this conversation. Here is material from it \
             that may be relevant — use it if it helps, but you don't have to:\n{context}\n\n\
             Student's question: {}",
            analysis.cleaned_query
        ),
        None => analysis.cleaned_query.clone(),
    };

    let history = build_history(&thread.messages);
    // Independent-audit finding (2026-08-05, reopens deferred.md #20):
    // state.http_client's blanket 120s .timeout() is a TOTAL request
    // deadline, not a per-chunk one — it would still kill a healthy,
    // continuously-streaming generation at the 120s wall-clock mark
    // regardless of this function's own CHUNK_INACTIVITY_TIMEOUT below.
    // streaming_http_client uses .read_timeout() instead, reqwest's
    // actual per-chunk-reset primitive — see state.rs's own comment.
    let chunks = ai_client::generate_stream(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        think,
        history,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<TurnEvent>(16);
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
                        if tx.send(TurnEvent::Delta(delta)).await.is_err() {
                            cutoff_reason = Some("cancelled by user".to_string());
                            break;
                        }
                    }
                    Ok(Some(Err(err))) => {
                        let reason = format!("ai service error mid-stream: {err}");
                        // deferred.md #53: best-effort — if the receiver
                        // is already gone (client disconnected right as
                        // this fired), there's nothing left to tell; the
                        // reason still lands in the staged audit event
                        // below regardless.
                        let _ = tx.send(TurnEvent::Error(reason.clone())).await;
                        cutoff_reason = Some(reason);
                        break;
                    }
                    Ok(None) => break, // stream ended normally — full response received
                    Err(_) => {
                        let reason = format!(
                            "generation stalled — no output for {}s",
                            CHUNK_INACTIVITY_TIMEOUT.as_secs()
                        );
                        let _ = tx.send(TurnEvent::Error(reason.clone())).await;
                        cutoff_reason = Some(reason);
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
