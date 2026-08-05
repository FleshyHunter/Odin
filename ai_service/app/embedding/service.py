from functools import lru_cache

from sentence_transformers import SentenceTransformer

from app.cache.service import EMBED_TTL_SECONDS, get_cached, set_cached

# Locked model (ARCHITECTURE.md, Technology Stack) — must stay identical
# at ingest and query time; changing it means re-embedding every stored
# chunk, so this name is not a config value, it's a fixed constant.
MODEL_NAME = "all-MiniLM-L6-v2"


@lru_cache(maxsize=1)
def get_model() -> SentenceTransformer:
    return SentenceTransformer(MODEL_NAME)


def embed_texts(texts: list[str]) -> list[list[float]]:
    """Redis Phase 3 (deferred.md #9-11): each text is checked against
    the cache individually (the model itself is fixed/locked, so an
    identical text always embeds to the identical vector) — only actual
    misses get encoded, still as ONE batched model.encode() call rather
    than one per miss, so a partial-cache-hit batch doesn't lose the
    batching efficiency this function existed to provide in the first
    place."""
    results: list[list[float] | None] = [get_cached("embed", text) for text in texts]
    missing_indices = [i for i, cached in enumerate(results) if cached is None]

    if missing_indices:
        model = get_model()
        missing_texts = [texts[i] for i in missing_indices]
        fresh = model.encode(missing_texts, convert_to_numpy=True).tolist()
        for index, vector in zip(missing_indices, fresh):
            results[index] = vector
            set_cached("embed", texts[index], vector, EMBED_TTL_SECONDS)

    return results  # type: ignore[return-value]
