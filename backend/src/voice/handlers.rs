use axum::extract::{Multipart, State};
use axum::Json;
use serde::Serialize;

use crate::ai_client;
use crate::auth::middleware::AuthUser;
use crate::state::AppState;

use super::errors::VoiceError;

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
/// as `ephemeral` uploads never touching the DB either.
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

    let text = ai_client::transcribe(&state.http_client, &state.ai_service_url, file_bytes, &filename).await?;
    Ok(Json(TranscribeResponse { text }))
}
