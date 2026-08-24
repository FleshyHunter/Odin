from fastapi import APIRouter, Form, HTTPException, UploadFile
from pydantic import BaseModel
from starlette.concurrency import run_in_threadpool

from app.voice.service import transcribe_audio

router = APIRouter()


class TranscribeResponse(BaseModel):
    text: str


@router.post("/transcribe", response_model=TranscribeResponse)
async def transcribe(
    file: UploadFile,
    # deferred.md #98 — optional, only sent by the chunked live-caption
    # path (Rust's stream_chunk). Absent (None) for the one-shot upload
    # and the final authoritative call on stop, which both need the
    # complete transcript, not a windowed one.
    window_seconds: float | None = Form(None),
) -> TranscribeResponse:
    # Must stay `async def` for the genuinely-async `await file.read()`
    # below, unlike /generate's plain `def` (which Starlette auto-
    # offloads to a threadpool). That means the actual blocking work —
    # transcribe_audio()'s call into Whisper — needs an EXPLICIT
    # threadpool hop instead, or it stalls the whole ASGI event loop for
    # the full inference duration on every call, blocking every other
    # concurrent ai_service request (health checks, /generate,
    # /analyze_input, another student's own transcription) until it
    # finishes. Was a rare one-shot cost before this endpoint served
    # only single post-recording calls; now that Rust's chunked-
    # streaming voice flow calls this repeatedly (every ~4s per active
    # recording — see backend/src/voice/handlers.rs), it's no longer
    # optional.
    audio_bytes = await file.read()
    try:
        text = await run_in_threadpool(
            transcribe_audio, audio_bytes, file.filename or "audio.webm", window_seconds
        )
    except Exception as e:
        raise HTTPException(status_code=422, detail=f"could not process audio file: {e}") from e
    return TranscribeResponse(text=text)
