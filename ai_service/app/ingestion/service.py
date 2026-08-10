import os
from dataclasses import dataclass, field

from app.embedding.service import embed_texts
from app.ingestion.chunk import chunk_text
from app.ingestion.extract import extract
from app.knowledge.service import GapBand, classify_topic_gap

# Ingestion Quality Gate's "minimum length filter" (PRD.md) — no
# numeric threshold was ever specified in any locked doc. 20 tokens is
# a judgment call for this pass: enough to rule out a near-empty or
# failed extraction (a title, a blank page), not meant as any kind of
# content-quality bar beyond that.
MIN_CONTENT_TOKENS = int(os.environ.get("MIN_CONTENT_TOKENS", "20"))


@dataclass
class IngestChunk:
    text: str
    token_count: int
    embedding: list[float]


@dataclass
class IngestResult:
    extracted_text: str
    rejected: bool
    rejection_reason: str | None
    chunks: list[IngestChunk] = field(default_factory=list)


SIZE_GUARDRAIL_MESSAGE = "This document is too large for temporary memoryless use. Start a track to ingest it properly."

# deferred.md #24 — not locked in any doc (PRD.md only says "Bad uploads
# rejected with a clear message," no exact wording given), a judgment
# call for this pass, same posture as SIZE_GUARDRAIL_MESSAGE above.
TOPIC_RELEVANCE_MESSAGE = (
    "This document doesn't seem related to what you're currently learning. "
    "Double-check it's the right file, or ask about it directly instead of "
    "uploading it as reference material."
)


def _is_topic_relevant(chunks: list[IngestChunk], subject_id: str, dag_version: int) -> bool:
    """deferred.md #24 — reuses classify_topic_gap (deferred.md #75/2b),
    no new detector or threshold. Document-level, not per-chunk: rejects
    only if EVERY chunk classifies OFF_TOPIC. If every chunk also comes
    back with best_concept_id=None, the subject has zero stored
    concept_embeddings (e.g. predates #75) — treat as relevant (can't
    determine), same "purely additive, never a false negative on its
    own" posture classify_topic_gap's own docstring already commits to.
    """
    saw_real_signal = False
    for chunk in chunks:
        signal = classify_topic_gap(chunk.embedding, subject_id, dag_version)
        if signal.best_concept_id is not None:
            saw_real_signal = True
        if signal.band != GapBand.OFF_TOPIC:
            return True
    return not saw_real_signal


def ingest(
    data: bytes,
    filename: str,
    role: str,
    max_chunks: int | None = None,
    subject_id: str | None = None,
    dag_version: int | None = None,
) -> IngestResult:
    """Runs the full ingestion pipeline: extract -> quality gate ->
    (ephemeral stops here) -> chunk -> chunk-count guardrail -> embed ->
    topic-relevance check.

    Deliberately does NOT touch content_hash/dedup (Rust's job, run
    BEFORE this is ever called — see Duplicate Upload Prevention) or
    Postgres/ChromaDB persistence (also Rust's job). This function only
    ever produces chunks + embeddings; what happens to them is entirely
    the caller's decision.

    subject_id/dag_version (deferred.md #24): only ever passed for
    journey-mode uploads (uploads/handlers.rs's "commit immediately"
    branch) — memoryless-mode uploads have no subject context at all, so
    the topic-relevance check below is skipped entirely when absent,
    same as it always has been.
    """
    extraction = extract(data, filename)
    if extraction.rejected:
        return IngestResult(extracted_text="", rejected=True, rejection_reason=extraction.rejection_reason)

    text = extraction.text.strip()

    if _token_count(text) < MIN_CONTENT_TOKENS:
        return IngestResult(
            extracted_text=text,
            rejected=True,
            rejection_reason="This document is too short to be useful — please upload more content.",
        )

    # Language check (English only) is NOT enforced here — same known,
    # already-tracked gap as chat-turn input (markdown/deferred.md #14,
    # resurfacing at this call site per #24). Not silently assumed
    # solved; genuinely unenforced.

    if role == "ephemeral":
        # Never chunked/embedded/stored, in any mode — PRD.md, Roles.
        return IngestResult(extracted_text=text, rejected=False, rejection_reason=None)

    chunk_texts = chunk_text(text)

    # deferred.md #47: checked HERE, before embed_texts() runs. Chunking
    # and embedding are two fully separate, sequential steps — the chunk
    # count is already known at this point, before the expensive GPU
    # call ever fires. This is what actually makes the guardrail run
    # "BEFORE embedding starts," as PRD.md's SIZE GUARDRAIL requires: the
    # original Rust-side-only check couldn't do this, since chunk_count
    # wasn't knowable until AFTER ai_service had already embedded
    # everything and returned.
    if max_chunks is not None and len(chunk_texts) > max_chunks:
        return IngestResult(extracted_text=text, rejected=True, rejection_reason=SIZE_GUARDRAIL_MESSAGE)

    embeddings = embed_texts(chunk_texts)
    chunks = [
        IngestChunk(text=t, token_count=_token_count(t), embedding=e)
        for t, e in zip(chunk_texts, embeddings)
    ]

    # deferred.md #24: only ever reachable for journey-mode uploads —
    # subject_id is never passed for memoryless staging or ephemeral.
    if subject_id is not None and dag_version is not None and not _is_topic_relevant(chunks, subject_id, dag_version):
        return IngestResult(extracted_text=text, rejected=True, rejection_reason=TOPIC_RELEVANCE_MESSAGE)

    return IngestResult(extracted_text=text, rejected=False, rejection_reason=None, chunks=chunks)


def _token_count(text: str) -> int:
    return len(text.split())
