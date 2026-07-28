import os
from dataclasses import dataclass, field

from app.embedding.service import embed_texts
from app.ingestion.chunk import chunk_text
from app.ingestion.extract import extract

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


def ingest(data: bytes, filename: str, role: str) -> IngestResult:
    """Runs the full ingestion pipeline: extract -> quality gate ->
    (ephemeral stops here) -> chunk -> embed.

    Deliberately does NOT touch content_hash/dedup (Rust's job, run
    BEFORE this is ever called — see Duplicate Upload Prevention) or
    Postgres/ChromaDB persistence (also Rust's job, since ChromaDB
    integration itself doesn't exist yet — markdown/deferred.md #25).
    This function only ever produces chunks + embeddings; what happens
    to them is entirely the caller's decision.
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
    #
    # Topic-relevance check is explicitly STUBBED, not silently
    # skipped — PRD.md names it with no stated mechanism, and a real
    # version would need subject/concept context that doesn't exist
    # yet (no real subject-creation flow — markdown/deferred.md #24).

    if role == "ephemeral":
        # Never chunked/embedded/stored, in any mode — PRD.md, Roles.
        return IngestResult(extracted_text=text, rejected=False, rejection_reason=None)

    chunk_texts = chunk_text(text)
    embeddings = embed_texts(chunk_texts)
    chunks = [
        IngestChunk(text=t, token_count=_token_count(t), embedding=e)
        for t, e in zip(chunk_texts, embeddings)
    ]
    return IngestResult(extracted_text=text, rejected=False, rejection_reason=None, chunks=chunks)


def _token_count(text: str) -> int:
    return len(text.split())
