import os
from dataclasses import dataclass

from app.knowledge.client import get_concept_embeddings, get_knowledge_global

# ARCHITECTURE.md's Environment Variables [LOCKED] list — both written
# as cosine-SIMILARITY values (not distance); see client.py's own note
# on why hnsw:space=cosine is what makes that arithmetic correct.
RETRIEVAL_MIN_SCORE = float(os.environ.get("RETRIEVAL_MIN_SCORE", "0.60"))
CHUNK_CONCEPT_MIN_SIMILARITY = float(os.environ.get("CHUNK_CONCEPT_MIN_SIMILARITY", "0.50"))


@dataclass
class ChunkRecord:
    """One knowledge_global document — ARCHITECTURE.md's ChromaDB
    Collection [LOCKED] schema exactly: id=chunk_id, embedding, and the
    8 named metadata fields below. subject_id/concept_id/journey_id are
    nullable (e.g. a material_upload chunk has no journey_id; an
    unassigned chunk has no concept_id — PRD.md's "misc chunk — still
    retrievable, just unassigned")."""

    chunk_id: str
    embedding: list[float]
    source_id: str
    upload_role: str
    trust_score: float
    subject_id: str | None = None
    concept_id: str | None = None
    journey_id: str | None = None
    difficulty: str | None = None
    chunk_type: str | None = None


def _metadata(record: ChunkRecord) -> dict:
    metadata = {
        "subject_id": record.subject_id,
        "concept_id": record.concept_id,
        "source_id": record.source_id,
        "journey_id": record.journey_id,
        "upload_role": record.upload_role,
        "trust_score": record.trust_score,
        "difficulty": record.difficulty,
        "chunk_type": record.chunk_type,
    }
    # Drop unset optional fields rather than sending an explicit null
    # through — filters behave more predictably against an absent key.
    return {key: value for key, value in metadata.items() if value is not None}


def add_chunks(records: list[ChunkRecord]) -> None:
    """Upserts real chunk documents into knowledge_global. This builds
    the write capability itself — wiring it into the real ingestion/
    conversion flow is separate, already-tracked work (deferred.md
    #17/#27), same "capability before caller" pattern already used for
    ai_client::generate_dag() and friends."""
    if not records:
        return
    get_knowledge_global().upsert(
        ids=[r.chunk_id for r in records],
        embeddings=[r.embedding for r in records],
        metadatas=[_metadata(r) for r in records],
    )


def add_concept_embedding(
    concept_id: str,
    subject_id: str,
    dag_version: int,
    title: str,
    embedding: list[float],
) -> None:
    """Upserts one concept_embeddings document — PRD.md's locked id
    format (concept_id:subject_id:dag_version) and metadata shape."""
    get_concept_embeddings().upsert(
        ids=f"{concept_id}:{subject_id}:{dag_version}",
        embeddings=embedding,
        metadatas={
            "concept_id": concept_id,
            "subject_id": subject_id,
            "dag_version": dag_version,
            "title": title,
        },
    )


@dataclass
class RetrievedChunk:
    chunk_id: str
    similarity: float
    metadata: dict


def _build_where(filters: dict | None) -> dict | None:
    """Chroma requires multi-condition filters explicitly wrapped in
    {"$and": [...]} — a plain multi-key dict raises ValueError
    ("Expected where to have exactly one operator...") (deferred.md
    #46, reproduced live against the real server: one key works, two+
    doesn't). Single-key filters pass through unwrapped since Chroma
    accepts that form directly and it's simpler to read in logs/tests."""
    if not filters:
        return None
    if len(filters) == 1:
        return filters
    return {"$and": [{key: value} for key, value in filters.items()]}


def query_knowledge_global(
    embedding: list[float],
    *,
    filters: dict | None = None,
    top_k: int = 10,
) -> list[RetrievedChunk]:
    """Metadata-filtered similarity search — ARCHITECTURE.md's Retrieval
    Rules: "Always filter metadata before/alongside similarity. Never
    raw cosine top-N alone." filters is any subset of {subject_id,
    concept_id, source_id, journey_id, upload_role, trust_score,
    difficulty, chunk_type} — converted to Chroma's `where` clause via
    _build_where (exact-match only — no locked caller needs range/
    comparison filters yet; multi-key filters get $and-wrapped, see
    _build_where's own docstring). Returns similarity (1 - cosine
    distance, so callers compare directly against RETRIEVAL_MIN_SCORE
    without re-deriving the conversion themselves; results already below
    that threshold are dropped here, since a sub-threshold result isn't
    really a "match" at all. Whether too FEW matches (< RETRIEVAL_MIN_
    CHUNKS) should trigger the acquisition fallback is the CALLER's
    decision, not this function's — it only returns what actually
    matched.

    SECURITY: `filters['journey_id']`, if present, must already be a
    Rust-verified, resource-ownership-checked value (ARCHITECTURE.md's
    "journey_id trust boundary" — ChromaDB has no RLS equivalent of its
    own and will search whatever journey_id it's given). This function
    trusts its caller completely, same as the rest of this AI gateway.
    """
    result = get_knowledge_global().query(
        query_embeddings=[embedding],
        n_results=top_k,
        where=_build_where(filters),
        include=["distances", "metadatas"],
    )
    ids = result["ids"][0]
    distances = result["distances"][0]
    metadatas = result["metadatas"][0]
    return [
        RetrievedChunk(chunk_id=chunk_id, similarity=1 - distance, metadata=metadata)
        for chunk_id, distance, metadata in zip(ids, distances, metadatas)
        if 1 - distance >= RETRIEVAL_MIN_SCORE
    ]


def assign_concept(embedding: list[float]) -> str | None:
    """Chunk-to-Concept Assignment (PRD.md): argmax cosine similarity
    against every concept_embeddings document; NULL (unassigned) if the
    best match is below CHUNK_CONCEPT_MIN_SIMILARITY, or if no concepts
    exist yet at all. Deliberately pure vector math — no LLM call, no
    token cost, deterministic (same chunk always maps to the same
    concept), matching PRD.md's own framing of this mechanism."""
    result = get_concept_embeddings().query(
        query_embeddings=[embedding],
        n_results=1,
        include=["distances", "metadatas"],
    )
    ids = result["ids"][0]
    if not ids:
        return None
    similarity = 1 - result["distances"][0][0]
    if similarity < CHUNK_CONCEPT_MIN_SIMILARITY:
        return None
    return result["metadatas"][0][0]["concept_id"]
