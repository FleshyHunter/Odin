from functools import lru_cache

from sentence_transformers import SentenceTransformer

# Locked model (ARCHITECTURE.md, Technology Stack) — must stay identical
# at ingest and query time; changing it means re-embedding every stored
# chunk, so this name is not a config value, it's a fixed constant.
MODEL_NAME = "all-MiniLM-L6-v2"


@lru_cache(maxsize=1)
def get_model() -> SentenceTransformer:
    return SentenceTransformer(MODEL_NAME)


def embed_texts(texts: list[str]) -> list[list[float]]:
    model = get_model()
    return model.encode(texts, convert_to_numpy=True).tolist()
