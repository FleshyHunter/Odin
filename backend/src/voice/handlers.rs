use std::convert::Infallible;

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
    let text =
        ai_client::transcribe(&state.streaming_http_client, &state.ai_service_url, file_bytes, &filename).await?;
    Ok(Json(TranscribeResponse { text }))
}

/// POST /voice/transcribe/stream — opened once per recording, before
/// any audio is uploaded. Yields a `session` event carrying a fresh
/// session_id the client then attaches to every subsequent
/// POST /voice/transcribe/chunk call, then forwards each chunk's
/// transcription result as a `partial`/`error` SSE event as it arrives.
/// Mirrors memoryless::handlers::send_message's mpsc-channel-fed
/// async_stream shape, but this stream itself does no transcription —
/// it only relays results pushed onto its channel by stream_chunk
/// calls on a SEPARATE connection (see voice/mod.rs's header comment
/// for why two connections rather than one duplex stream).
pub async fn stream_start(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::channel::<VoiceStreamEvent>(8);
    state.voice_sessions.lock().unwrap().insert(session_id, VoiceSession { user_id, tx });
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
/// client-side, not accumulated here). Stateless w.r.t. audio bytes:
/// looks up the session, re-transcribes the whole buffer, pushes the
/// result onto that session's channel, and returns immediately —
/// the actual result is delivered over the SSE connection opened by
/// stream_start, not in this response body.
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
    // guard before the transcribe() call.
    let owner_and_tx = {
        let sessions = state.voice_sessions.lock().unwrap();
        sessions.get(&session_id).map(|s| (s.user_id, s.tx.clone()))
    };

    let Some((owner_id, tx)) = owner_and_tx else {
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

    match ai_client::transcribe(&state.streaming_http_client, &state.ai_service_url, file_bytes, &filename).await {
        Ok(text) => {
            let _ = tx.send(VoiceStreamEvent::Partial(text)).await;
        }
        Err(err) => {
            let _ = tx.send(VoiceStreamEvent::Error(err.to_string())).await;
        }
    }

    Ok(StatusCode::ACCEPTED)
}
