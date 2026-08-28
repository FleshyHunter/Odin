use axum::extract::{Multipart, State};
use axum::Json;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::ai_client;
use crate::auth::middleware::AuthUser;
use crate::journeys;
use crate::memoryless::errors::MemorylessError;
use crate::memoryless::staging::{self, StagedChunk, StagedThread, StagedUpload};
use crate::memoryless::write_through;
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
    journey_id: Option<Uuid>,
}

async fn parse_multipart(mut multipart: Multipart) -> Result<ParsedUpload, MemorylessError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut role: Option<String> = None;
    let mut thread_id: Option<Uuid> = None;
    let mut journey_id: Option<Uuid> = None;

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
            // deferred.md #37 — mutually exclusive with thread_id: present
            // only for journey-mode's "commit immediately" branch below.
            Some("journey_id") => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| MemorylessError::Validation("invalid journey_id field".to_string()))?;
                if !text.is_empty() {
                    journey_id = Some(
                        Uuid::parse_str(&text).map_err(|_| MemorylessError::Validation("invalid journey_id".to_string()))?,
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

    Ok(ParsedUpload { file_bytes, filename, role, thread_id, journey_id })
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

// deferred.md #92: one file's worth of the exact same content_hash ->
// dedup -> ingest -> stage sequence `upload()` below already ran
// single-file — extracted so memoryless::handlers::send_message can run
// it in a loop (up to 5 attachments bundled into one turn) without
// duplicating this logic. `rejected` is a SOFT outcome here (the size
// guardrail tripped for THIS file) rather than an Err — `upload()` below
// still turns it into a hard request failure (its one-file-per-call
// contract), but a multi-file caller can report it via one file's own
// upload_result and continue processing the rest of the batch.
pub(crate) struct StagedFileOutcome {
    pub filename: String,
    pub deduped: bool,
    pub chunk_count: usize,
    pub extracted_text: String,
    pub rejected: Option<String>,
    // Some only for a real, newly-staged upload (rejected.is_none() &&
    // !deduped) — the exact identifier deferred.md #92's pending-
    // promotion mechanism needs to look this upload back up later by,
    // since filenames aren't guaranteed unique within one batch.
    pub content_hash: Option<String>,
}

pub(crate) async fn stage_one_file(
    state: &AppState,
    user_id: Uuid,
    thread: &mut StagedThread,
    file_bytes: Vec<u8>,
    filename: String,
    role: &str,
) -> Result<StagedFileOutcome, MemorylessError> {
    let max_bytes = state.memoryless_staged_upload_max_mb * 1024 * 1024;
    if file_bytes.len() as u64 > max_bytes {
        return Ok(StagedFileOutcome {
            filename,
            deduped: false,
            chunk_count: 0,
            extracted_text: String::new(),
            rejected: Some(SIZE_GUARDRAIL_MESSAGE.to_string()),
            content_hash: None,
        });
    }

    let content_hash = dedup::compute_content_hash(&file_bytes);

    if dedup::find_staged(thread, &content_hash).is_some()
        || dedup::find_committed(&state.pool, user_id, &content_hash).await?.is_some()
    {
        return Ok(StagedFileOutcome {
            filename,
            deduped: true,
            chunk_count: 0,
            extracted_text: String::new(),
            rejected: None,
            content_hash: None,
        });
    }

    let result = ai_client::ingest(
        &state.http_client,
        &state.ai_service_url,
        file_bytes,
        &filename,
        role,
        state.memoryless_staged_upload_max_chunks,
        None,
        None,
    )
    .await?;
    if result.rejected {
        return Ok(StagedFileOutcome {
            filename,
            deduped: false,
            chunk_count: 0,
            extracted_text: String::new(),
            rejected: Some(result.rejection_reason.unwrap_or_else(|| "upload rejected".to_string())),
            content_hash: None,
        });
    }

    let chunk_count = result.chunks.len();
    let staged_upload = StagedUpload {
        content_hash,
        filename: filename.clone(),
        upload_role: role.to_string(),
        extracted_text: result.extracted_text.clone(),
        chunks: result
            .chunks
            .into_iter()
            .map(|c| StagedChunk { text: c.text, token_count: c.token_count, embedding: c.embedding })
            .collect(),
        created_at: Utc::now(),
    };
    thread.staged_uploads.push(staged_upload.clone());

    // Same fail-soft posture as upload()'s own identical call below —
    // the upload already succeeded from the student's point of view
    // (staged, usable this turn) even if this durability write fails.
    if staged_upload.upload_role == "material_upload"
        && let Err(err) = write_through::write_through_material_upload(state, user_id, &staged_upload, None).await
    {
        tracing::error!(?err, thread_id = %thread.thread_id, "failed to write material_upload through to Postgres");
    }

    Ok(StagedFileOutcome {
        filename,
        deduped: false,
        chunk_count,
        extracted_text: result.extracted_text,
        rejected: None,
        content_hash: Some(staged_upload.content_hash),
    })
}

/// POST /uploads — handles all three roles (PRD.md, Roles: "How do you
/// want to use this document?"). Ephemeral: extract and return text
/// only, no persistence, no thread needed at all. prompt_upload/
/// material_upload: content_hash computed HERE (before ai_service ever
/// runs), deduped, then EITHER staged into a memoryless thread
/// (`thread_id` present or absent, the original path) OR — deferred.md
/// #37 — committed immediately with no Redis staging at all
/// (`journey_id` present, PRD.md Flow 1: "JOURNEY MODE: store
/// immediately (ChromaDB + PostgreSQL)"). The two params are mutually
/// exclusive; a real caller only ever sends one.
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
        let result = ai_client::ingest(
            &state.http_client,
            &state.ai_service_url,
            parsed.file_bytes,
            &parsed.filename,
            &parsed.role,
            state.memoryless_staged_upload_max_chunks,
            None,
            None,
        )
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

    if let Some(journey_id) = parsed.journey_id {
        return upload_to_journey(state, user_id, journey_id, parsed).await;
    }

    let mut thread = match parsed.thread_id {
        Some(id) => staging::load_owned(&state, id, user_id).await?,
        None => StagedThread::new(Uuid::new_v4(), user_id),
    };

    let outcome =
        stage_one_file(&state, user_id, &mut thread, parsed.file_bytes, parsed.filename, &parsed.role).await?;
    if let Some(reason) = outcome.rejected {
        return Err(MemorylessError::Validation(reason));
    }
    // deferred.md #45: when parsed.thread_id was None, `thread` above is
    // a brand-new StagedThread that has never been written to Redis —
    // without a save, the thread_id handed back here is a "phantom"
    // that any follow-up call would 410 on. Always saved here (covers
    // both the real-new-upload path and the deduped-refresh path),
    // unlike stage_one_file itself, which never saves — a multi-file
    // caller wants exactly one save after its whole batch, not one per
    // file.
    staging::save(&state, &thread).await?;

    Ok(Json(UploadResponse {
        thread_id: Some(thread.thread_id),
        extracted_text: outcome.extracted_text,
        deduped: outcome.deduped,
        chunk_count: outcome.chunk_count,
    }))
}

/// deferred.md #37 — journey-mode's own upload path, deliberately
/// separate from the memoryless staging flow above rather than
/// threaded through it with extra branches: no Redis at all here (PRD.md
/// Flow 1, "store immediately"), a different dedup scope
/// (`find_committed_in_journey`, not `find_committed` — the same file
/// can legitimately exist in two different journeys), and a real
/// `subject_id`/`dag_version` now flow into `ai_client::ingest()`,
/// which is what actually makes deferred.md #24's topic-relevance check
/// live. `thread_id` in the response is always `None` — there is no
/// staging thread to point back at, same shape as the `ephemeral` case
/// above.
async fn upload_to_journey(
    state: AppState,
    user_id: Uuid,
    journey_id: Uuid,
    parsed: ParsedUpload,
) -> Result<Json<UploadResponse>, MemorylessError> {
    let subject_id = journeys::turn::verify_journey_and_subject(&state.pool, user_id, journey_id).await?;
    let dag_version: i32 = sqlx::query_scalar("SELECT dag_version FROM subjects WHERE subject_id = $1")
        .bind(subject_id)
        .fetch_one(&state.pool)
        .await?;

    let content_hash = dedup::compute_content_hash(&parsed.file_bytes);
    let already_committed = if parsed.role == "prompt_upload" {
        dedup::find_committed_in_journey(&state.pool, user_id, journey_id, &content_hash).await?
    } else {
        dedup::find_committed(&state.pool, user_id, &content_hash).await?
    };
    if already_committed.is_some() {
        return Ok(Json(UploadResponse { thread_id: None, extracted_text: String::new(), deduped: true, chunk_count: 0 }));
    }

    let result = ai_client::ingest(
        &state.http_client,
        &state.ai_service_url,
        parsed.file_bytes,
        &parsed.filename,
        &parsed.role,
        state.memoryless_staged_upload_max_chunks,
        Some(subject_id),
        Some(dag_version),
    )
    .await?;
    if result.rejected {
        return Err(MemorylessError::Validation(
            result.rejection_reason.unwrap_or_else(|| "upload rejected".to_string()),
        ));
    }

    let chunk_count = result.chunks.len();
    let staged_upload = StagedUpload {
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
    };

    // Unlike memoryless mode's fail-soft write-through, both branches
    // here propagate errors (`?`) — there is no Redis copy backing this
    // upload as a fallback, so a failed Postgres/ChromaDB write means
    // the upload genuinely didn't succeed, not just "durability is
    // pending" — the caller needs to know and retry.
    if staged_upload.upload_role == "prompt_upload" {
        write_through::write_through_prompt_upload(&state, user_id, journey_id, subject_id, &staged_upload).await?;
    } else {
        // Some(journey_id): tags this upload's origin so
        // journeys::turn::fetch_journey_upload_context finds it for
        // THIS journey — its global visibility is unaffected either way.
        write_through::write_through_material_upload(&state, user_id, &staged_upload, Some(journey_id)).await?;
    }

    Ok(Json(UploadResponse {
        thread_id: None,
        extracted_text: result.extracted_text,
        deduped: false,
        chunk_count,
    }))
}
