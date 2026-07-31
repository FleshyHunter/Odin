import json

from fastapi import APIRouter
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from app.generation.service import generate_text, generate_text_stream

router = APIRouter()

# English Only (PRD.md, RULES.md — locked requirement; deferred.md #14,
# previously unenforced after langdetect was tried and removed for
# misreading short strings). Folded into these two endpoints specifically
# — NOT into generate_text()/generate_text_stream() themselves — so
# intent/classifier.py's own direct qwen-fallback call (which bypasses
# this router entirely) is unaffected; only real chat-turn responses are
# gated. The model's own language understanding replaces the old
# statistical detector: no separate library, no extra model call (this
# is the SAME generation call that was always going to run), and no
# streaming impact (the model decides while writing, not after the fact
# — see the "which system prompt" discussion this was built from).
_ENGLISH_ONLY_SYSTEM_PROMPT = (
    "If the student's message is not written in English, do not answer "
    "their question — respond only with: \"I can only help in English "
    "right now — please rephrase your question in English.\" Otherwise, "
    "answer normally."
)


class GenerateRequest(BaseModel):
    prompt: str
    think: bool = True


class GenerateResponse(BaseModel):
    response: str


@router.post("/generate", response_model=GenerateResponse)
def generate(request: GenerateRequest) -> GenerateResponse:
    return GenerateResponse(
        response=generate_text(request.prompt, request.think, system=_ENGLISH_ONLY_SYSTEM_PROMPT)
    )


# NDJSON (one JSON object per line), not SSE — this is the FastAPI->Rust
# hop only (Rule 9: Rust is the sole AI-gateway boundary), and NDJSON is
# simpler to parse there than SSE's framing. Rust re-exposes this to the
# browser as real SSE separately (backend/src/memoryless), which is
# where a proper Server-Sent-Events format actually earns its keep.
#
# Two possible lines: {"delta": "..."} for each text chunk, or exactly
# one {"error": "..."} as the LAST line if generation fails partway —
# the HTTP status is already 200 by the time any of this is streaming,
# so a status code can't carry a mid-stream failure; this is the only
# way to signal one to the caller.
def _stream_lines(prompt: str, think: bool):
    try:
        for delta in generate_text_stream(prompt, think, system=_ENGLISH_ONLY_SYSTEM_PROMPT):
            yield json.dumps({"delta": delta}) + "\n"
    except Exception as exc:  # noqa: BLE001 - deliberately broad: whatever
        # Ollama/the client raises here must still reach the caller as a
        # clean final line, not a bare disconnected stream.
        yield json.dumps({"error": str(exc)}) + "\n"


@router.post("/generate/stream")
def generate_stream(request: GenerateRequest) -> StreamingResponse:
    return StreamingResponse(
        _stream_lines(request.prompt, request.think),
        media_type="application/x-ndjson",
    )
