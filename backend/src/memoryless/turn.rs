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
    // deferred.md #92: one per file bundled into this turn's Send,
    // emitted before any Delta — lets the client show per-file progress
    // instead of a silent wait while up to 5 files extract/chunk/embed.
    UploadResult { filename: String, chunk_count: usize, deduped: bool, error: Option<String> },
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

// deferred.md #102: split from CHUNK_INACTIVITY_TIMEOUT above — a single
// 120s bound applied to the WAIT FOR THE VERY FIRST chunk too, which is
// unsafe now that generate_stream_with_tools() can silently spend a
// pre-first-chunk round-trip (decide -> call tool -> get result ->
// resume) before yielding anything. This project's own observed
// single-call generation times run 22s-120s+ under load, so a two-call
// tool sequence's worst case is genuinely in the 200s+ range — sized
// generously here rather than tightly. Applied to EVERY turn through
// this loop, not only tool-using ones: the underlying risk (a
// legitimately slow first token under load, mistaken for a stall) was
// already latent for plain think=True turns too, so this closes that
// gap as well, at zero cost to the common fast case. Once streaming has
// actually started, CHUNK_INACTIVITY_TIMEOUT still applies unchanged —
// this only widens the PRE-first-chunk wait.
const FIRST_CHUNK_TIMEOUT: Duration = Duration::from_secs(300);

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

// deferred.md #92: exact-string-only classifier — found LIVE (2026-08-28,
// real end-to-end test) that a prefix-matching version of this ("yes"
// followed by any trailing content, minus a negation-adjacency check)
// wrongly promoted a file when the student replied "yes let's go
// through problem 2" — answering an entirely different, later question,
// not the promotion offer, which the model had correctly decided NOT
// to even ask about that turn. journeys/turn.rs's is_branch_confirmed
// independently hit and closed this exact bug class the same day (its
// own doc comment has the full history: a negation-adjacency check,
// then a trailing-word allowlist, both kept reproducing the same bug at
// a different grain size) — mirrored here rather than re-deriving:
// a closed list of complete, exact strings, matched only via `==`, plus
// a '?' guard (a question is never a confirmation). Known, accepted
// trade-off, same as that entry's: a real confirmation phrased outside
// this list declines, same as any other unmatched reply — silently
// promoting something the student didn't actually agree to is the
// worse failure mode of the two, not a declined promotion that's
// trivially re-offered next time an upload comes in.
const PROMOTION_AFFIRMATIVE: &[&str] = &[
    "yes", "yeah", "yep", "yup", "sure", "ok", "okay", "alright", "please", "go for it", "do it",
    "sounds good", "lets do it", "let's do it", "please do", "sure thing", "yeah sure",
];

fn is_affirmative(text: &str) -> bool {
    if text.contains('?') {
        return false;
    }
    let normalized: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c == '\'')
        .collect();
    PROMOTION_AFFIRMATIVE.contains(&normalized.as_str())
}

#[cfg(test)]
mod is_affirmative_tests {
    use super::*;

    #[test]
    fn every_affirmative_entry_confirms() {
        for phrase in PROMOTION_AFFIRMATIVE {
            assert!(is_affirmative(phrase), "{phrase:?} should confirm");
        }
    }

    #[test]
    fn plain_declines_do_not_confirm() {
        for text in ["no", "not now", "nope", "nah", "don't"] {
            assert!(!is_affirmative(text), "{text:?} should not confirm");
        }
    }

    // The actual live-found bug this whole rewrite exists for: a "yes"
    // that's really answering something else entirely must not confirm
    // just because it starts with an affirmative word.
    #[test]
    fn an_affirmative_word_followed_by_unrelated_content_does_not_confirm() {
        for text in ["yes let's go through problem 2", "yeah, no thanks", "sure, not now", "ok, nope"] {
            assert!(!is_affirmative(text), "{text:?} should not confirm");
        }
    }

    #[test]
    fn a_question_never_confirms_even_if_it_starts_with_an_affirmative_word() {
        assert!(!is_affirmative("yes, but does that even matter?"));
    }
}

/// Starts a streaming chat turn. Runs analyze_input and opens the
/// generate() stream synchronously (so an unreachable ai_service still
/// surfaces as a normal, immediate error to the caller — see
/// ai_client::generate_stream's own doc comment) — only once that
/// succeeds does this spawn the background task and return the
/// receiving end for the caller to relay onward as SSE.
pub async fn start_turn_stream(
    state: &AppState,
    mut thread: StagedThread,
    raw_input: String,
    think: bool,
    upload_outcomes: Vec<crate::uploads::handlers::StagedFileOutcome>,
) -> Result<mpsc::Receiver<TurnEvent>, MemorylessError> {
    // deferred.md #92: resolve a PRIOR turn's "want this added to your
    // library?" question first — same "check pending state at the start
    // of the next turn" placement journeys/turn.rs uses for
    // pending_branch_topic. Cleared unconditionally either way; a
    // decline/ambiguous reply is silent (no extra framing), same
    // precedent as a declined branch offer — the message still gets
    // processed as a normal turn below regardless of which way this goes.
    let mut extra_instruction: Option<String> = None;
    if !thread.pending_promotion_hashes.is_empty() {
        let pending = std::mem::take(&mut thread.pending_promotion_hashes);
        if is_affirmative(&raw_input) {
            let mut promoted_any = false;
            for hash in &pending {
                if let Some(upload) = thread.staged_uploads.iter().find(|u| &u.content_hash == hash).cloned() {
                    match write_through::write_through_material_upload(state, thread.user_id, &upload, None).await {
                        Ok(()) => promoted_any = true,
                        Err(err) => {
                            tracing::error!(?err, thread_id = %thread.thread_id, filename = %upload.filename, "failed to promote staged upload to the shared library")
                        }
                    }
                }
            }
            if promoted_any {
                extra_instruction = Some(
                    "(You just added the student's file(s) to their permanent library, per their \
                     confirmation — briefly acknowledge this, then continue naturally.)"
                        .to_string(),
                );
            }
        }
    }

    // deferred.md #92: a brand-new upload with no accompanying text is a
    // legitimate turn on its own — the tutor looks at what was just
    // extracted and, only if it seems durably reusable, asks (in its own
    // reply) about adding it to the permanent library. One combined
    // question for the whole batch, not one per file. Skipped when the
    // block above already set an acknowledgment — a turn resolves at
    // most one of "ask" or "just promoted," not both.
    if extra_instruction.is_none() {
        let new_hashes: Vec<String> = upload_outcomes.iter().filter_map(|o| o.content_hash.clone()).collect();
        if !new_hashes.is_empty() && raw_input.trim().is_empty() {
            thread.pending_promotion_hashes = new_hashes;
            extra_instruction = Some(
                "(The student just shared one or more files with no accompanying message. Look at \
                 what was just extracted above and respond naturally to it. If any of it seems like \
                 something genuinely worth keeping permanently — not just for this one conversation \
                 — ask, in your own words, whether they'd like it added to their permanent library. \
                 Otherwise just treat it as context for this conversation and continue naturally; \
                 don't ask if it doesn't seem to warrant it.)"
                    .to_string(),
            );
        }
    }

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
    //
    // deferred.md #92: extra_instruction is server-authored, never user
    // content — kept outside both tag pairs, same distinction journeys/
    // turn.rs's own send_journey_message already draws for its own
    // extra_instruction (branch/fold-gap nudges).
    let mut prompt_parts: Vec<String> = Vec::new();
    if !context_blocks.is_empty() {
        prompt_parts.push(format!("<reference_material>\n{}\n</reference_material>", context_blocks.join("\n\n")));
    }
    if let Some(instruction) = &extra_instruction {
        prompt_parts.push(instruction.clone());
    }
    let prompt = if prompt_parts.is_empty() {
        analysis.cleaned_query.clone()
    } else if context_blocks.is_empty() {
        format!("{}\n\nStudent's question: {}", prompt_parts.join("\n\n"), analysis.cleaned_query)
    } else {
        prompt_parts.push(format!("<student_message>\n{}\n</student_message>", analysis.cleaned_query));
        prompt_parts.join("\n\n")
    };

    let history = build_history(&thread.messages);
    // Independent-audit finding (2026-08-05, reopens deferred.md #20):
    // state.http_client's blanket 120s .timeout() is a TOTAL request
    // deadline, not a per-chunk one — it would still kill a healthy,
    // continuously-streaming generation at the 120s wall-clock mark
    // regardless of this function's own CHUNK_INACTIVITY_TIMEOUT below.
    // streaming_http_client uses .read_timeout() instead, reqwest's
    // actual per-chunk-reset primitive — see state.rs's own comment.
    // deferred.md #101/#102: this is the one normal-message path in
    // memoryless mode — the tools-enabled endpoint, now safe against
    // FIRST_CHUNK_TIMEOUT above covering a pre-first-chunk tool
    // round-trip.
    let call_start = std::time::Instant::now();
    let chunks = ai_client::generate_stream_with_tools(
        &state.streaming_http_client,
        &state.ai_service_url,
        prompt,
        think,
        history,
    )
    .await?;

    let (tx, rx) = mpsc::channel::<TurnEvent>(16);

    // deferred.md #92: sent before any Delta, so a multi-file batch shows
    // per-file progress instead of a silent wait — best-effort, same as
    // every other tx.send() in this file; a client that's already gone
    // just means nothing was listening.
    for outcome in &upload_outcomes {
        let _ = tx
            .send(TurnEvent::UploadResult {
                filename: outcome.filename.clone(),
                chunk_count: outcome.chunk_count,
                deduped: outcome.deduped,
                error: outcome.rejected.clone(),
            })
            .await;
    }

    let state = state.clone();
    let cleaned_query = analysis.cleaned_query;
    let matched_concepts = analysis.matched_concepts;
    let detected_intent = analysis.detected_intent;
    let thread_id = thread.thread_id;

    tokio::spawn(async move {
        let mut thread = thread;
        let mut accumulated = String::new();
        let mut cutoff_reason: Option<String> = None;
        // deferred.md #60: same first-chunk-arrival signal as
        // journeys/turn.rs's stream_to_completion — tracked inline here
        // since this loop isn't shared with that helper (separate module).
        let mut first_chunk_at: Option<std::time::Instant> = None;

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
                    // deferred.md #102: FIRST_CHUNK_TIMEOUT until the
                    // first delta has ever arrived, then the tighter
                    // CHUNK_INACTIVITY_TIMEOUT for every gap after that —
                    // first_chunk_at (already tracked below for the
                    // queued_ms metric) is exactly the right signal to
                    // key this off, no separate tracking needed.
                    result = {
                        let timeout_duration =
                            if first_chunk_at.is_none() { FIRST_CHUNK_TIMEOUT } else { CHUNK_INACTIVITY_TIMEOUT };
                        tokio::time::timeout(timeout_duration, chunks.next())
                    } => {
                        match result {
                            Ok(Some(Ok(delta))) => {
                                first_chunk_at.get_or_insert_with(std::time::Instant::now);
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
                                // first_chunk_at is still None here (a
                                // Some would mean a delta already arrived
                                // and reset the wait) — safe to re-derive
                                // which bound just fired from that same
                                // signal, rather than threading the
                                // chosen duration out of the block above.
                                let fired = if first_chunk_at.is_none() { FIRST_CHUNK_TIMEOUT } else { CHUNK_INACTIVITY_TIMEOUT };
                                let reason = format!(
                                    "generation stalled — no output for {}s",
                                    fired.as_secs()
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

        let end = std::time::Instant::now();
        // deferred.md #60: no chunk ever arrived (immediate error/cutoff)
        // — all elapsed time was spent "queued" in the sense that zero
        // generation ever happened, generation_ms is genuinely 0.
        let queued_ms = (first_chunk_at.unwrap_or(end) - call_start).as_millis() as i32;
        let generation_ms = first_chunk_at.map(|t| (end - t).as_millis() as i32).unwrap_or(0);

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
            queued_ms,
            generation_ms,
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
