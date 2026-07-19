from fastapi import FastAPI

from app.embedding.router import router as embedding_router
from app.embedding.service import get_model

app = FastAPI(title="Odin AI Service")

app.include_router(embedding_router)


@app.get("/health")
def health() -> dict:
    # cache_info() only introspects the lru_cache — it never triggers a
    # model load, so this stays a cheap liveness check either way.
    model_loaded = get_model.cache_info().currsize > 0
    return {"status": "ok", "embedding_model_loaded": model_loaded}
