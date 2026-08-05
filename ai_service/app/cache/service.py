import hashlib
import json
import os
from functools import lru_cache
from typing import Any

import redis

# Redis Phase 3 (ARCHITECTURE.md, Redis Phasing — "mature" phase):
# embeddings/retrieval/spaCy/OCR results, keyed by input hash. Only
# embed() and the spaCy step of analyze_input() actually have real
# callers today (deferred.md #9-11) — retrieval (query_knowledge_global)
# has zero callers anywhere yet (deferred.md #18, still unwired) and OCR
# has no code at all (deferred.md #22, KIV), so neither is wired to this
# cache — that would be caching a function nothing ever calls.
#
# Optional, not required: unlike OLLAMA_HOST/CHROMADB_HOST (hard
# `os.environ[...]`, fail-fast-at-boot), a missing/unreachable Redis
# here just means caching is silently off — every call recomputes fresh,
# same result, just slower. Caching is a pure performance optimization
# (this is literally Redis Phasing's LAST, "mature" phase), never
# something a caller should fail without.
REDIS_URL = os.environ.get("REDIS_URL")

# Locked ranges (ARCHITECTURE.md, Redis Phasing): "embed 7-30d, retrieval
# 1-24h, NLP 24h, DAG 30d, OCR 30d." Using the upper bound of each range
# — nothing here is fed by a live-changing source (the embedding model
# and spaCy pipeline are both fixed once deployed), so the longer end of
# each range is the more useful default; still a real TTL, not permanent.
EMBED_TTL_SECONDS = 30 * 24 * 3600
NLP_TTL_SECONDS = 24 * 3600


@lru_cache(maxsize=1)
def _get_client() -> "redis.Redis | None":
    if not REDIS_URL:
        return None
    return redis.Redis.from_url(REDIS_URL, decode_responses=True)


def _cache_key(namespace: str, raw: str) -> str:
    digest = hashlib.sha256(raw.encode("utf-8")).hexdigest()
    return f"cache:{namespace}:{digest}"


def get_cached(namespace: str, raw: str) -> Any | None:
    """Returns the cached JSON-decoded value for this namespace+input,
    or None on either a real cache miss or caching being disabled/Redis
    being unreachable — callers can't tell the difference and don't
    need to, since both cases mean exactly the same thing: compute it
    fresh."""
    client = _get_client()
    if client is None:
        return None
    try:
        cached = client.get(_cache_key(namespace, raw))
    except redis.RedisError:
        return None
    if cached is None:
        return None
    return json.loads(cached)


def set_cached(namespace: str, raw: str, value: Any, ttl_seconds: int) -> None:
    """Best-effort — a failed write here should never fail the request
    that computed `value` in the first place; it just means this result
    won't be cached this time."""
    client = _get_client()
    if client is None:
        return
    try:
        client.set(_cache_key(namespace, raw), json.dumps(value), ex=ttl_seconds)
    except redis.RedisError:
        pass
