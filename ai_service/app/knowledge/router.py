from fastapi import APIRouter
from pydantic import BaseModel

from app.knowledge.service import ChunkRecord, add_chunks, query_knowledge_global

router = APIRouter()


class QueryKnowledgeGlobalRequest(BaseModel):
    embedding: list[float]
    # The only filter exposed at the wire level (deferred.md #18) —
    # matches ARCHITECTURE.md's "One collection supports both
    # subject-scoped retrieval ... and cross-subject tangent retrieval
    # (no subject filter)". query_knowledge_global() itself stays fully
    # general (any subset of the 8 locked filter fields) for whenever a
    # second caller needs more than this.
    subject_id: str | None = None
    top_k: int = 5


class RetrievedChunkOut(BaseModel):
    chunk_id: str
    similarity: float
    metadata: dict


class QueryKnowledgeGlobalResponse(BaseModel):
    chunks: list[RetrievedChunkOut]


@router.post("/knowledge/query", response_model=QueryKnowledgeGlobalResponse)
def query(request: QueryKnowledgeGlobalRequest) -> QueryKnowledgeGlobalResponse:
    filters = {"subject_id": request.subject_id} if request.subject_id else None
    results = query_knowledge_global(request.embedding, filters=filters, top_k=request.top_k)
    return QueryKnowledgeGlobalResponse(
        chunks=[
            RetrievedChunkOut(chunk_id=r.chunk_id, similarity=r.similarity, metadata=r.metadata)
            for r in results
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
