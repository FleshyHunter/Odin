// Chat-turn orchestration: receive one raw message -> analyze_input
// (Block 8) -> STREAM a generate() reply (Blocks 5/6, Block 11 follow-up
// — see markdown/deferred.md #20) -> append both message sides plus one
// audit event to the staged thread once the stream ends. Still BARE
// against journey state specifically — no mastery_bank reads/writes, no
// prerequisite checks, no completion-check logic — memoryless mode has
// no journey/subject to scope any of that against. NOT bare against
// retrieval, though: THIS thread's own staged uploads (deferred.md #19,
// see all_staged_context — always included, unconditionally) AND the
// permanent global knowledge base (deferred.md #18, see
// query_knowledge_context, still similarity-gated) are both wired in,
// independently of each other and of one another's success/failure.
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
use super::staging::{self, StagedAuditEvent, StagedMessage, StagedThread, StagedUpload};
use super::write_through;
use crate::ai_client::{self, AiClientError};
use crate::knowledge;
use crate::state::AppState;
use uuid::Uuid;

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

// Found live (2026-08-12): the previous similarity-gated design
// (rank staged-upload chunks against the CURRENT message's embedding,
// only include what scored >= RETRIEVAL_MIN_SCORE) silently dropped a
// student's own just-uploaded document from the prompt whenever their
// question was ABOUT the document rather than semantically similar TO
// it — measured live, a completely ordinary "how do I answer this
// worksheet" scored 0.44 against its own worksheet's content, well
// below the 0.60 floor. That's the normal case, not an edge case.
// Replaced with unconditional inclusion: everything staged in this
// thread, any role, every turn — no embedding call, no threshold, no
// failure mode (pure, synchronous). Token cost is accepted knowingly,
// not something to work around with an arbitrary cap.
fn all_staged_context(staged_uploads: &[StagedUpload]) -> Option<String> {
    let texts: Vec<&str> = staged_uploads
        .iter()
        .flat_map(|upload| upload.chunks.iter())
        .map(|chunk| chunk.text.as_str())
        .collect();

    if texts.is_empty() {
        return None;
    }
    Some(texts.join("\n\n"))
}

// deferred.md #18: the other half of retrieval — the permanent, shared
// ChromaDB knowledge_global collection, as opposed to all_staged_
// context's THIS-thread-only staged uploads above. Deliberately
// independent of it (its own embed() call, not a shared one) — touching
// #19's already-shipped, live-verified internals to shave one cheap
// /embed round trip wasn't worth the risk against the multi-second
// generation call this whole turn is actually bottlenecked on. Unscoped
// (no subject_id) — memoryless mode has no subject/journey context,
// matching ARCHITECTURE.md's "cross-subject tangent retrieval" case.
// Fails open on any embedding-service error — this one's still a
// genuine retrieval ENHANCEMENT (broad KB search), not core, unlike
// all_staged_context above which is now unconditional.
async fn query_knowledge_context(state: &AppState, user_id: Uuid, query: &str) -> Result<Option<String>, AiClientError> {
    let query_embedding = ai_client::embed(&state.http_client, &state.ai_service_url, vec![query.to_string()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AiClientError::UnexpectedResponse("embed() returned no vectors for the query".to_string()))?;

    knowledge::query_global_context(state, user_id, &query_embedding, None, None).await
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
        None,
        None,
        None,
        None,
    )
    .await?;

    // deferred.md #19: staged-upload context — see all_staged_context's
    // own doc comment. Scoped to THIS thread's own staged chunks only,
    // always included, no ranking, no failure mode.
    let retrieved_context = all_staged_context(&thread.staged_uploads);

    // deferred.md #18: the permanent, shared knowledge base — see
    // query_knowledge_context's own doc comment. Independent of the
    // staged-upload retrieval above (runs regardless of whether this
    // thread has any staged uploads at all).
    let global_context = match query_knowledge_context(state, thread.user_id, &analysis.cleaned_query).await {
        Ok(context) => context,
        Err(err) => {
            tracing::warn!(?err, thread_id = %thread.thread_id, "knowledge-base retrieval failed, continuing without it");
            None
        }
    };

    let mut context_blocks: Vec<String> = Vec::new();
    if let Some(context) = &retrieved_context {
        context_blocks.push(format!(
            "The student uploaded this document to this conversation. Treat it as reference material \
             for what they're asking about:\n{context}"
        ));
    }
    if let Some(context) = &global_context {
        context_blocks.push(format!(
            "Here is material from the knowledge base that may be relevant — use it if it helps, \
             but you don't have to:\n{context}"
        ));
    }
    // deferred.md #91: tags, not just a "Student's question:" prose
    // label — found live that the model has no reliable way to tell
    // injected reference material apart from the student's own message
    // in one concatenated blob (a document's own content, e.g. an
    // unusual test marker phrase, got misjudged as the STUDENT writing
    // non-English, refusing a plainly-English question). _SYSTEM_PROMPT
    // (ai_service/app/generation/router.py) now points its language
    // check at these exact tag names — keep both in sync if either
    // changes. No wrapping needed when there's no reference material at
    // all — nothing for the model to conflate the query with.
    let prompt = if context_blocks.is_empty() {
        analysis.cleaned_query.clone()
    } else {
        format!(
            "<reference_material>\n{}\n</reference_material>\n\n<student_message>\n{}\n</student_message>",
            context_blocks.join("\n\n"),
            analysis.cleaned_query
        )
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
                tokio::select! {
                    // Checked FIRST (biased — plain, unordered select
                    // would let an already-ready chunk win the race
                    // half the time even after cancellation) and
                    // independently of chunks.next() below. Found live,
                    // this session: the OLD code only ever noticed a
                    // cancelled turn reactively, via THIS SAME tx.send()
                    // failing — but that only runs once chunks.next()
                    // actually yields something. For a "thinking" turn
                    // where ai_service/Ollama takes a while to produce
                    // even its first token, the loop was fully blocked
                    // on chunks.next() the entire time, unable to notice
                    // a cancellation that had already happened up to 13+
                    // seconds earlier (axum itself drops the connection
                    // within ~1s of the client disconnecting — confirmed
                    // live — so the delay was entirely this gap, not a
                    // slow disconnect signal). tx.closed() resolves the
                    // instant the receiver (owned by the SSE stream in
                    // handlers.rs) is dropped, independent of whether
                    // anything is currently flowing through the channel.
                    biased;
                    _ = tx.closed() => {
                        cutoff_reason = Some("cancelled by user".to_string());
                        break;
                    }
                    result = tokio::time::timeout(CHUNK_INACTIVITY_TIMEOUT, chunks.next()) => {
                        match result {
                            Ok(Some(Ok(delta))) => {
                                accumulated.push_str(&delta);
                                // tx.closed() above already covers the
                                // cancellation case for any turn that's
                                // actually producing output — this
                                // remains as a harmless fallback for the
                                // rare timing where both branches raced
                                // ready at once and this one still won.
                                if tx.send(TurnEvent::Delta(delta)).await.is_err() {
                                    cutoff_reason = Some("cancelled by user".to_string());
                                    break;
                                }
                            }
                            Ok(Some(Err(err))) => {
                                let reason = format!("ai service error mid-stream: {err}");
                                // deferred.md #53: best-effort — if the
                                // receiver is already gone (client
                                // disconnected right as this fired),
                                // there's nothing left to tell; the
                                // reason still lands in the staged audit
                                // event below regardless.
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
            }
        }

        let now = Utc::now();
        let user_message = StagedMessage { role: "user".to_string(), content: raw_input.clone(), timestamp: now };
        let tutor_message =
            StagedMessage { role: "tutor".to_string(), content: accumulated.clone(), timestamp: now };
        thread.messages.push(user_message.clone());
        thread.messages.push(tutor_message.clone());

        // One audit event per turn (Rule 4: mandatory every turn, all
        // modes) regardless of how it ended — staged here rather than
        // written to audit_logs directly, per Rule 51.
        let audit_event = StagedAuditEvent {
            user_input: raw_input,
            cleaned_query,
            matched_concepts,
            detected_intent,
            response_text: accumulated,
            model_used: MODEL_USED.to_string(),
            error: cutoff_reason,
            timestamp: now,
        };
        thread.audit_events.push(audit_event.clone());

        if let Err(err) = staging::save(&state, &thread).await {
            tracing::error!(?err, %thread_id, "failed to stage memoryless turn after streaming");
        }

        // deferred.md #56: incremental Postgres write-through, right
        // alongside the Redis staging above — fail-soft on purpose. A
        // write-through failure never blocks or unwinds a turn the
        // student already saw complete; it only means THIS turn's
        // durability rests on Redis alone until a later turn's own
        // write-through call catches up (each call is independent, no
        // retry needed — see write_through.rs's own doc comment).
        if let Err(err) = write_through::write_through_turn(
            &state.pool,
            thread.user_id,
            thread_id,
            thread.created_at,
            &[user_message, tutor_message],
            &audit_event,
        )
        .await
        {
            tracing::error!(?err, %thread_id, "failed to write memoryless turn through to Postgres");
        }
    });

    Ok(rx)
}
