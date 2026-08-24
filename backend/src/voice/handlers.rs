use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::Stream;
use serde::Serialize;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ai_client;
use crate::auth::middleware::AuthUser;
use crate::state::AppState;

use super::errors::VoiceError;
use super::session::{VoiceSession, VoiceSessionRegistry, VoiceStreamEvent};

// deferred.md #98 — how much trailing audio each chunk re-transcribes,
// not the whole growing recording. Starting hypothesis, not locked
// (same posture as voice/mod.rs's own CHUNK_INTERVAL_MS on the
// frontend): long enough to give Whisper real sentence-level context
// so it doesn't mishear a word cut off right at the window's edge,
// short enough to keep re-transcription cost roughly flat regardless
// of total recording length — the actual problem this fixes.
const CHUNK_WINDOW_SECONDS: f32 = 10.0;

/// deferred.md #98 — finds where `current`'s content picks up relative
/// to `previous`, and returns only the words from that point on.
///
/// Two genuinely different alignments are possible, depending on
/// whether the sliding window (CHUNK_WINDOW_SECONDS) has started
/// actually sliding yet:
///   - Early in a recording (total audio so far <= the window size),
///     BOTH `previous` and `current` still cover the recording from
///     its true start — they share a common PREFIX from index 0 (a
///     later tick is that same prefix plus more words, or a revision
///     partway through it).
///   - Once the recording runs longer than the window, `current` no
///     longer starts at the recording's true beginning — it starts
///     somewhere in the middle of what `previous` covered. There, the
///     real overlap is `previous`'s TAIL matching `current`'s HEAD, not
///     a shared prefix from 0 — a plain common-prefix check would find
///     a mismatch at word zero and (wrongly) treat the entire new
///     window as new, duplicating whatever the two windows still
///     genuinely share. Found and fixed before this ever shipped, not
///     live — hand-traced a 3-tick sequence where an older word ages
///     out of the window and confirmed a suffix-only-unaware version
///     double-appends.
///
/// Rather than tracking which regime a given tick is in, this checks
/// both alignments and takes whichever finds the larger overlap —
/// correct either way, and cheap: word counts here are tiny (a ~10s
/// window), so trying both is irrelevant cost-wise. Whisper doesn't
/// guarantee identical wording for the same audio twice, so a revised
/// word can shrink whichever overlap is found (more gets re-sent as
/// "new" than strictly necessary) — an accepted trade-off, not a bug:
/// a live transcript is already best-effort, and re-sending a few
/// already-correct words is far better than silent duplication.
fn diff_new_suffix(previous: &str, current: &str) -> String {
    let previous_words: Vec<&str> = previous.split_whitespace().collect();
    let current_words: Vec<&str> = current.split_whitespace().collect();
    let max_overlap = previous_words.len().min(current_words.len());

    let prefix_overlap =
        previous_words.iter().zip(current_words.iter()).take_while(|(a, b)| a == b).count();

    let sliding_overlap = (0..=max_overlap)
        .rev()
        .find(|&k| previous_words[previous_words.len() - k..] == current_words[..k])
        .unwrap_or(0);

    let overlap = prefix_overlap.max(sliding_overlap);
    current_words[overlap..].join(" ")
}

#[derive(Serialize)]
pub struct TranscribeResponse {
    text: String,
}

/// POST /voice/transcribe (deferred.md #80) — the real backend half of
/// the locked Voice Input flow (ARCHITECTURE_LOCK.md, Upload System —
/// Voice Input: mic tap -> MediaRecorder captures audio -> this route
/// -> text drops into the composer input box for the student to review,
/// never auto-sent). Deliberately bare: no persistence, no thread
/// association at all — nothing here has any business writing to
/// Postgres/Redis, matching the locked flow's own scope exactly, same
/// as `ephemeral` uploads never touching the DB either. Also the
/// authoritative FINAL call for chunked-streaming recordings (see
/// stream_start/stream_chunk below) — always re-run once more here on
/// stop rather than reusing the last streamed partial, since it's the
/// only way to guarantee the complete buffer (including whatever
/// MediaRecorder flushed right at stop()) was actually transcribed.
pub async fn transcribe(
    AuthUser(_user_id): AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<TranscribeResponse>, VoiceError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| VoiceError::Validation("invalid multipart body".to_string()))?
    {
        if field.name() == Some("file") {
            filename = field.file_name().map(|s| s.to_string());
            let bytes = field
                .bytes()
                .await
                .map_err(|_| VoiceError::Validation("could not read audio file".to_string()))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| VoiceError::Validation("missing file".to_string()))?;
    // MediaRecorder-produced audio has no reliable original filename —
    // ai_client::transcribe() only uses this for its own temp-file
    // extension (see its own doc comment), so a fixed name matching the
    // real recorded format is correct here, not a placeholder.
    let filename = filename.unwrap_or_else(|| "recording.webm".to_string());

    // streaming_http_client, not http_client: see its own doc comment
    // in state.rs. A single transcribe call is ordinarily well inside
    // http_client's 120s total deadline, but chunked-streaming recording
    // (stream_chunk below) can put sustained, legitimate load on the
    // same shared Whisper instance this call also depends on — using
    // the read-timeout client here too means a slow-but-still-
    // progressing response is never falsely killed.
    let text = ai_client::transcribe(&state.streaming_http_client, &state.ai_service_url, file_bytes, &filename, None)
        .await?;
    Ok(Json(TranscribeResponse { text }))
}

/// POST /voice/transcribe/stream — opened once per recording, before
/// any audio is uploaded. Yields a `session` event carrying a fresh
/// session_id the client then attaches to every subsequent
/// POST /voice/transcribe/chunk call, then forwards each chunk's NEW
/// words (deferred.md #98 — a diff against the previous chunk's
/// windowed transcript, not the full text) as a `partial`/`error` SSE
/// event as it arrives — the client is expected to APPEND `partial`'s
/// payload to whatever it's already shown, not replace it. Mirrors
/// memoryless::handlers::send_message's mpsc-channel-fed async_stream
/// shape, but this stream itself does no transcription — it only
/// relays results pushed onto its channel by stream_chunk calls on a
/// SEPARATE connection (see voice/mod.rs's header comment for why two
/// connections rather than one duplex stream).
pub async fn stream_start(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<VoiceStreamEvent>(8);
    state
        .voice_sessions
        .lock()
        .unwrap()
        .insert(session_id, VoiceSession { user_id, tx, last_transcript: Arc::new(Mutex::new(String::new())) });
    let registry = state.voice_sessions.clone();

    let sse_stream = async_stream::stream! {
        // Removes this session's registry entry the instant this stream
        // ends, for ANY reason — normal completion (the loop below
        // exhausts because stream_chunk's sender side was dropped) OR
        // early client disconnect (axum drops this whole future then).
        // Without this, a session whose client vanished mid-recording
        // would leak in the registry forever — mirrors turn.rs's own
        // cleanup-via-Drop idiom rather than relying on a happy-path-only
        // cleanup call.
        struct RegistryGuard {
            session_id: Uuid,
            registry: VoiceSessionRegistry,
        }
        impl Drop for RegistryGuard {
            fn drop(&mut self) {
                self.registry.lock().unwrap().remove(&self.session_id);
            }
        }
        let _guard = RegistryGuard { session_id, registry };

        yield Ok(Event::default().event("session").data(session_id.to_string()));
        while let Some(event) = rx.recv().await {
            match event {
                VoiceStreamEvent::Partial(text) => yield Ok(Event::default().event("partial").data(text)),
                VoiceStreamEvent::Error(reason) => yield Ok(Event::default().event("error").data(reason)),
            }
        }
        yield Ok(Event::default().event("done").data("ok"));
    };

    Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

/// POST /voice/transcribe/chunk — fired every ~4s by the client with
/// the FULL growing audio buffer re-uploaded each time (not just the
/// new bytes — WebM's header only exists in the first MediaRecorder
/// chunk, so a decodable file needs everything from the start; see
/// voice/mod.rs's header comment for why that buffer lives
/// client-side, not accumulated here). Stateless w.r.t. audio BYTES —
/// still no persistence — but NOT stateless w.r.t. transcript text
/// (deferred.md #98): only the last CHUNK_WINDOW_SECONDS of the buffer
/// actually gets re-transcribed (bounds cost regardless of total
/// recording length), and the session's own `last_transcript` is what
/// lets this handler diff that windowed result against the previous
/// tick's, so only genuinely new words get pushed onto the session's
/// channel — the actual result is delivered over the SSE connection
/// opened by stream_start, not in this response body.
pub async fn stream_chunk(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, VoiceError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut session_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| VoiceError::Validation("invalid multipart body".to_string()))?
    {
        match field.name() {
            Some("file") => {
                filename = field.file_name().map(|s| s.to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| VoiceError::Validation("could not read audio file".to_string()))?;
                file_bytes = Some(bytes.to_vec());
            }
            Some("session_id") => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| VoiceError::Validation("invalid session_id field".to_string()))?;
                session_id = Uuid::parse_str(&text).ok();
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| VoiceError::Validation("missing file".to_string()))?;
    let filename = filename.unwrap_or_else(|| "recording.webm".to_string());
    let session_id =
        session_id.ok_or_else(|| VoiceError::Validation("missing or invalid session_id".to_string()))?;

    // Scoped block: never hold a std::sync::MutexGuard across an
    // .await point below — it isn't Send, so the surrounding future
    // wouldn't be either. Clone out just what's needed and drop the
    // guard before the transcribe() call. last_transcript is its own
    // Arc<Mutex<_>> specifically so it can be held onto independently
    // of this outer registry lock (see session.rs's own comment).
    let owner_tx_last = {
        let sessions = state.voice_sessions.lock().unwrap();
        sessions.get(&session_id).map(|s| (s.user_id, s.tx.clone(), s.last_transcript.clone()))
    };

    let Some((owner_id, tx, last_transcript)) = owner_tx_last else {
        // Session already closed (recording stopped and its SSE
        // connection torn down right as this chunk landed) or never
        // existed — not an error. The final authoritative
        // /voice/transcribe call on stop doesn't depend on this chunk
        // succeeding.
        return Ok(StatusCode::ACCEPTED);
    };
    if owner_id != user_id {
        // Don't reveal whether the session_id exists for another user.
        return Err(VoiceError::Validation("session not found".to_string()));
    }

    match ai_client::transcribe(
        &state.streaming_http_client,
        &state.ai_service_url,
        file_bytes,
        &filename,
        Some(CHUNK_WINDOW_SECONDS),
    )
    .await
    {
        Ok(windowed_text) => {
            // deferred.md #98: diff against what this session last sent,
            // then store this tick's full windowed text as the new
            // baseline for the NEXT tick's diff — never send the diff
            // payload itself as the stored baseline, ai_service always
            // returns the full current window, not an increment.
            let new_words = {
                let mut previous = last_transcript.lock().unwrap();
                let diff = diff_new_suffix(&previous, &windowed_text);
                *previous = windowed_text;
                diff
            };
            if !new_words.is_empty() {
                let _ = tx.send(VoiceStreamEvent::Partial(new_words)).await;
            }
        }
        Err(err) => {
            let _ = tx.send(VoiceStreamEvent::Error(err.to_string())).await;
        }
    }

    Ok(StatusCode::ACCEPTED)
}

#[cfg(test)]
mod tests {
    use super::diff_new_suffix;

    #[test]
    fn first_tick_sends_everything() {
        assert_eq!(diff_new_suffix("", "hello world"), "hello world");
    }

    #[test]
    fn only_new_words_after_a_matching_prefix() {
        assert_eq!(
            diff_new_suffix("integrate x times e to the x", "integrate x times e to the x plus one"),
            "plus one"
        );
    }

    #[test]
    fn no_new_words_when_unchanged() {
        assert_eq!(diff_new_suffix("same words here", "same words here"), "");
    }

    #[test]
    fn a_revised_word_at_the_seam_shifts_the_diff_point_earlier() {
        // "e" revised to "eigenvalues" — the mismatch means the common
        // prefix stops one word earlier than the textually-longest
        // overlap, and the whole revised tail is treated as new.
        // Documents the accepted trade-off from diff_new_suffix's own
        // doc comment.
        assert_eq!(diff_new_suffix("solve for e", "solve for eigenvalues now"), "eigenvalues now");
    }

    #[test]
    fn sliding_window_drops_the_earliest_word_without_duplicating() {
        // The exact bug this function was rewritten to fix: "hello" has
        // aged out of the window by this tick, so current no longer
        // shares a prefix with previous at all — but "world nice to
        // meet you" is still real overlap (previous's tail = current's
        // head), and only "today" is genuinely new.
        assert_eq!(
            diff_new_suffix("hello world nice to meet you", "world nice to meet you today"),
            "today"
        );
    }
}
