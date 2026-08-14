from fastapi import APIRouter
from pydantic import BaseModel

from app.knowledge.service import (
    OVER_FETCH_MULTIPLIER,
    ChunkRecord,
    final_rank_score,
    add_chunks,
    add_concept_embedding,
    query_knowledge_global,
)

router = APIRouter()


class QueryKnowledgeGlobalRequest(BaseModel):
    embedding: list[float]
    # The only filters exposed at the wire level (deferred.md #18) —
    # matches ARCHITECTURE.md's "One collection supports both
    # subject-scoped retrieval ... and cross-subject tangent retrieval
    # (no subject filter)". query_knowledge_global() itself stays fully
    # general (any subset of the 8 locked filter fields) for whenever a
    # second caller needs more than this.
    subject_id: str | None = None
    # deferred.md #18's own privacy gap, found 2026-08-12: this
    # retrieval previously only ever filtered by subject_id — since
    # prompt_upload is supposed to be journey-private but two different
    # students' journeys can share a subject_id, one student's private
    # upload could leak into another student's journey. journey_id is
    # journey-mode's own real, ownership-verified value (never trusted
    # from the client directly — Rust resolves it via
    # verify_journey_and_subject before this call ever fires).
    journey_id: str | None = None
    top_k: int = 5


class RetrievedChunkOut(BaseModel):
    chunk_id: str
    similarity: float
    metadata: dict


class QueryKnowledgeGlobalResponse(BaseModel):
    chunks: list[RetrievedChunkOut]


@router.post("/knowledge/query", response_model=QueryKnowledgeGlobalResponse)
def query(request: QueryKnowledgeGlobalRequest) -> QueryKnowledgeGlobalResponse:
    """deferred.md #18 privacy fix (2026-08-12, confirmed with V3 before
    building): two separate queries, not one filter, mirroring
    sources_mixed_scope's own RLS policy shape exactly
    (`upload_role='material_upload' OR (upload_role='prompt_upload' AND
    journey_id=X)`) rather than risking a novel Chroma `$or`-filter
    expression (this project already hit one real filter bug, #46).

    Query A — material_upload only, subject-scoped (or fully unscoped
    for memoryless mode's own cross-subject search): the shared, global
    pool, always searched, unaffected by journey_id. Explicitly excludes
    prompt_upload — without this, subject-scoped search would keep
    surfacing ANY journey's private uploads, the exact bug being fixed.

    Query B — prompt_upload only, subject-scoped AND journey_id-scoped:
    only runs when journey_id is present. journey_id is only ever set on
    a prompt_upload chunk's own Chroma metadata in the first place (see
    write_chunks_to_knowledge_global's own doc comment), so this
    naturally can never match a material_upload chunk even without an
    explicit upload_role filter here too — added anyway, for a filter
    that documents its own intent rather than relying on that fact
    silently holding.

    Over-fetches from EACH query (top_k * OVER_FETCH_MULTIPLIER, the
    same margin query_knowledge_global itself uses) before merging —
    truncating each query to top_k independently first would bias
    toward whichever tier has more raw matches rather than genuinely
    higher-ranked ones. final_rank_score is recomputed at merge time
    (not carried on RetrievedChunk) since it's a pure function of
    (similarity, metadata), both already present on every result.
    """
    fetch_k = request.top_k * OVER_FETCH_MULTIPLIER

    material_filters: dict = {"upload_role": "material_upload"}
    if request.subject_id:
        material_filters["subject_id"] = request.subject_id
    results = list(query_knowledge_global(request.embedding, filters=material_filters, top_k=fetch_k))

    if request.journey_id:
        prompt_filters: dict = {"upload_role": "prompt_upload", "journey_id": request.journey_id}
        if request.subject_id:
            prompt_filters["subject_id"] = request.subject_id
        results += query_knowledge_global(request.embedding, filters=prompt_filters, top_k=fetch_k)

    ranked = sorted(results, key=lambda r: final_rank_score(r.similarity, r.metadata), reverse=True)
    return QueryKnowledgeGlobalResponse(
        chunks=[
            RetrievedChunkOut(chunk_id=r.chunk_id, similarity=r.similarity, metadata=r.metadata)
            for r in ranked[: request.top_k]
        ]
    )


# deferred.md #18b — the write half's HTTP wrapper. Field names/shape
# mirror ChunkRecord exactly (ARCHITECTURE.md's ChromaDB Collection
# [LOCKED] schema: id=chunk_id, embedding, 8 named metadata fields).
class ChunkRecordIn(BaseModel):
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


class AddChunksRequest(BaseModel):
    records: list[ChunkRecordIn]


class AddChunksResponse(BaseModel):
    added: int


@router.post("/knowledge/add_chunks", response_model=AddChunksResponse)
def add_chunks_endpoint(request: AddChunksRequest) -> AddChunksResponse:
    records = [ChunkRecord(**record.model_dump()) for record in request.records]
    add_chunks(records)
    return AddChunksResponse(added=len(records))


# deferred.md #75/2b — populates concept_embeddings, the collection
# add_concept_embedding() has always been able to write to but that
# nothing has ever called (same "capability before caller" gap #18b
# closed for knowledge_global). One call site: journeys::service's
# persist_new_subject, right after a brand-new subject's concepts commit.
class AddConceptEmbeddingRequest(BaseModel):
    concept_id: str
    subject_id: str
    dag_version: int
    title: str
    embedding: list[float]


class AddConceptEmbeddingResponse(BaseModel):
    added: bool


@router.post("/knowledge/add_concept_embedding", response_model=AddConceptEmbeddingResponse)
def add_concept_embedding_endpoint(request: AddConceptEmbeddingRequest) -> AddConceptEmbeddingResponse:
    add_concept_embedding(
        request.concept_id, request.subject_id, request.dag_version, request.title, request.embedding
    )
    return AddConceptEmbeddingResponse(added=True)
