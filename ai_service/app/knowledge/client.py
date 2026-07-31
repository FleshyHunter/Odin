import os
from functools import lru_cache
from urllib.parse import urlparse

import chromadb
from chromadb.api import ClientAPI
from chromadb.api.models.Collection import Collection

# Milestone 9 (ARCHITECTURE.md, "ChromaDB Collection [LOCKED — Single
# Global Collection]"): ONE shared collection for all chunk documents
# (not per-subject), plus a second, separate collection for concept-
# level embeddings — PRD.md is explicit these must never mix (a past
# ambiguous draft that suggested storing concept embeddings in
# knowledge_global was corrected specifically because that would
# violate this collection's own chunk-documents-only lock).
KNOWLEDGE_GLOBAL = "knowledge_global"
CONCEPT_EMBEDDINGS = "concept_embeddings"

# Read at import time, not lazily inside get_client() — same "fail
# clearly, fail early" pattern as OLLAMA_HOST (generation/service.py).
_CHROMADB_HOST = os.environ["CHROMADB_HOST"]


@lru_cache(maxsize=1)
def get_client() -> ClientAPI:
    # CHROMADB_HOST is a full URL (matches OLLAMA_HOST/AI_SERVICE_URL's
    # own convention in ARCHITECTURE.md's Environment Variables list),
    # but chromadb.HttpClient wants host/port/ssl as separate arguments
    # — parsed here rather than inventing a second, differently-shaped
    # env var just for this one client.
    parsed = urlparse(_CHROMADB_HOST)
    return chromadb.HttpClient(
        host=parsed.hostname or _CHROMADB_HOST,
        port=parsed.port or 8000,
        ssl=parsed.scheme == "https",
    )


def _get_or_create(name: str) -> Collection:
    # embedding_function=None and hnsw:space=cosine are both deliberate,
    # not defaults left alone:
    #   - embedding_function=None: every caller in this module always
    #     supplies its own precomputed all-MiniLM-L6-v2 embedding (via
    #     ai_service's existing /embed path). Leaving Chroma's own
    #     default embedding function enabled would let it silently
    #     embed with a DIFFERENT model the moment anyone forgot to pass
    #     one, violating the locked "same embedding model at ingest AND
    #     query time" rule (AI Gateway Pattern) with no error at all.
    #   - hnsw:space=cosine: Chroma's own default is squared L2
    #     distance, not cosine. PRD.md's RETRIEVAL_MIN_SCORE/
    #     CHUNK_CONCEPT_MIN_SIMILARITY thresholds are both written as
    #     cosine SIMILARITY values (0.60, 0.50) — confirmed live against
    #     the real Windows ChromaDB that this setting makes
    #     `1 - distance` equal cosine similarity (identical vectors ->
    #     distance 0.0, opposite vectors -> distance 2.0); the default
    #     L2 space would silently make every threshold comparison
    #     meaningless instead of erroring.
    return get_client().get_or_create_collection(
        name,
        metadata={"hnsw:space": "cosine"},
        embedding_function=None,
    )


def get_knowledge_global() -> Collection:
    return _get_or_create(KNOWLEDGE_GLOBAL)


def get_concept_embeddings() -> Collection:
    return _get_or_create(CONCEPT_EMBEDDINGS)
