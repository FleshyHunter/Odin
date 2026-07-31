use axum::extract::{Multipart, State};
use axum::Json;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::ai_client;
use crate::auth::middleware::AuthUser;
use crate::memoryless::errors::MemorylessError;
use crate::memoryless::staging::{self, StagedChunk, StagedThread, StagedUpload};
use crate::state::AppState;

use super::dedup;

const VALID_ROLES: [&str; 3] = ["ephemeral", "prompt_upload", "material_upload"];

// PRD.md, Memoryless Mode — SIZE GUARDRAIL (deferred.md #32, previously
// unenforced despite the doc already framing this as "closes a real
// gap"). Exact locked message text — shown regardless of which limit
// (MB or chunk count) was actually exceeded, same as the doc specifies.
const SIZE_GUARDRAIL_MESSAGE: &str =
    "This document is too large for temporary memoryless use. Start a track to ingest it properly.";

struct ParsedUpload {
    file_bytes: Vec<u8>,
    filename: String,
    role: String,
    thread_id: Option<Uuid>,
}

async fn parse_multipart(mut multipart: Multipart) -> Result<ParsedUpload, MemorylessError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut role: Option<String> = None;
    let mut thread_id: Option<Uuid> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| MemorylessError::Validation("invalid multipart body".to_string()))?
    {
        match field.name() {
            Some("file") => {
                filename = field.file_name().map(|s| s.to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| MemorylessError::Validation("could not read file".to_string()))?;
                file_bytes = Some(bytes.to_vec());
            }
            Some("role") => {
                role = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| MemorylessError::Validation("invalid role field".to_string()))?,
                );
            }
            Some("thread_id") => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| MemorylessError::Validation("invalid thread_id field".to_string()))?;
                if !text.is_empty() {
                    thread_id = Some(
                        Uuid::parse_str(&text).map_err(|_| MemorylessError::Validation("invalid thread_id".to_string()))?,
                    );
                }
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| MemorylessError::Validation("missing file".to_string()))?;
    let filename = filename.unwrap_or_else(|| "upload".to_string());
    let role = role.ok_or_else(|| MemorylessError::Validation("missing role".to_string()))?;
    if !VALID_ROLES.contains(&role.as_str()) {
        return Err(MemorylessError::Validation(format!("invalid role: {role}")));
    }

    Ok(ParsedUpload { file_bytes, filename, role, thread_id })
}

#[derive(Serialize)]
pub struct UploadResponse {
    // None for ephemeral — never staged, so there's no thread to point
    // back at (PRD.md, Roles: "no stored version to collide with").
    thread_id: Option<Uuid>,
    extracted_text: String,
    deduped: bool,
    chunk_count: usize,
}

/// POST /uploads — handles all three roles (PRD.md, Roles: "How do you
/// want to use this document?"). Ephemeral: extract and return text
/// only, no persistence, no thread needed at all. prompt_upload/
/// material_upload: content_hash computed HERE (before ai_service ever
/// runs), deduped against three scopes, then staged into a memoryless
/// thread — journey-mode's "commit immediately" path is NOT built in
/// this pass, since no real journey-mode conversation flow exists yet
/// to occur "during" (no Flow 2, no journey creation — see markdown/
/// deferred.md #4/#17/#27); every real upload today happens in a
/// memoryless conversation, so staging is the only reachable path.
pub async fn upload(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<UploadResponse>, MemorylessError> {
    let parsed = parse_multipart(multipart).await?;

    // Enforced BEFORE embedding starts (PRD.md's own wording) — checked
    // against every role, not just the staged ones: an oversized
    // ephemeral upload would still run the full extract/OCR/chunk/embed
    // pipeline for nothing, which is exactly the wasted-compute problem
    // this guardrail exists to prevent.
    let max_bytes = state.memoryless_staged_upload_max_mb * 1024 * 1024;
    if parsed.file_bytes.len() as u64 > max_bytes {
        return Err(MemorylessError::Validation(SIZE_GUARDRAIL_MESSAGE.to_string()));
    }

    if parsed.role == "ephemeral" {
        let result = ai_client::ingest(&state.http_client, &state.ai_service_url, parsed.file_bytes, &parsed.filename, &parsed.role)
            .await?;
        if result.rejected {
            return Err(MemorylessError::Validation(
                result.rejection_reason.unwrap_or_else(|| "upload rejected".to_string()),
            ));
        }
        return Ok(Json(UploadResponse {
            thread_id: None,
            extracted_text: result.extracted_text,
            deduped: false,
            chunk_count: 0,
        }));
    }

    let content_hash = dedup::compute_content_hash(&parsed.file_bytes);

    let mut thread = match parsed.thread_id {
        Some(id) => staging::load_owned(&state, id, user_id).await?,
        None => StagedThread::new(Uuid::new_v4(), user_id),
    };

    if dedup::find_staged(&thread, &content_hash).is_some() {
        return Ok(Json(UploadResponse {
            thread_id: Some(thread.thread_id),
            extracted_text: String::new(),
            deduped: true,
            chunk_count: 0,
        }));
    }
    if dedup::find_committed(&state.pool, user_id, &content_hash).await?.is_some() {
        return Ok(Json(UploadResponse {
            thread_id: Some(thread.thread_id),
            extracted_text: String::new(),
            deduped: true,
            chunk_count: 0,
        }));
    }

    let result = ai_client::ingest(&state.http_client, &state.ai_service_url, parsed.file_bytes, &parsed.filename, &parsed.role)
        .await?;
    if result.rejected {
        return Err(MemorylessError::Validation(
            result.rejection_reason.unwrap_or_else(|| "upload rejected".to_string()),
        ));
    }

    let chunk_count = result.chunks.len();
    // Only knowable AFTER chunking (content-dependent, unlike the byte-
    // size check above) — rejected here, before ever staging into
    // Redis, rather than after (no point saving something we're about
    // to tell the user is too large).
    if chunk_count > state.memoryless_staged_upload_max_chunks {
        return Err(MemorylessError::Validation(SIZE_GUARDRAIL_MESSAGE.to_string()));
    }
    thread.staged_uploads.push(StagedUpload {
        content_hash,
        filename: parsed.filename,
        upload_role: parsed.role,
        extracted_text: result.extracted_text.clone(),
        chunks: result
            .chunks
            .into_iter()
            .map(|c| StagedChunk { text: c.text, token_count: c.token_count, embedding: c.embedding })
            .collect(),
        created_at: Utc::now(),
    });
    staging::save(&state, &thread).await?;

    Ok(Json(UploadResponse {
        thread_id: Some(thread.thread_id),
        extracted_text: result.extracted_text,
        deduped: false,
        chunk_count,
    }))
}
