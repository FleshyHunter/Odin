import json

from fastapi import APIRouter
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from app.generation.service import generate_text, generate_text_stream

router = APIRouter()


class GenerateRequest(BaseModel):
    prompt: str
    think: bool = True


class GenerateResponse(BaseModel):
    response: str


@router.post("/generate", response_model=GenerateResponse)
def generate(request: GenerateRequest) -> GenerateResponse:
    return GenerateResponse(response=generate_text(request.prompt, request.think))


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
        for delta in generate_text_stream(prompt, think):
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
