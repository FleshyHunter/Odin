import logging

from fastapi import FastAPI

from app.analyze_input.router import router as analyze_input_router
from app.embedding.router import router as embedding_router
from app.embedding.service import get_model
from app.generation.router import router as generation_router
from app.grading.router import router as grading_router
from app.voice.router import router as voice_router

# Root logger defaults to WARNING with no handler attached — verified
# live that intent/classifier.py's logger.info() call was silently
# dropped without this, in both a bare logging call and a real request
# that reached it. uvicorn configures its OWN uvicorn/uvicorn.access/
# uvicorn.error loggers, but never touches the root logger or any
# app-namespaced one, so this is the actual, necessary fix — not just
# where the one call site lives.
logging.basicConfig(level=logging.INFO)

app = FastAPI(title="Odin AI Service")

app.include_router(embedding_router)
app.include_router(generation_router)
app.include_router(voice_router)
app.include_router(analyze_input_router)
app.include_router(grading_router)


@app.get("/health")
def health() -> dict:
    # cache_info() only introspects the lru_cache — it never triggers a
    # model load, so this stays a cheap liveness check either way.
    model_loaded = get_model.cache_info().currsize > 0
    return {"status": "ok", "embedding_model_loaded": model_loaded}
